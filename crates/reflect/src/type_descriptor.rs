use std::any::{Any, TypeId, type_name};

use pixui_base::erased_value::{SendValue, SendValues};
use pixui_base::{PixuiResult, pixui_bail, pixui_error};

use crate::construction::SendConstructor;
use crate::method::Invocation;
use crate::{DynamicObject, Field, FieldIndex, Method, MethodIndex};
use crate::{SequenceDescriptor, TypeKind};

/// Immutable registration for a concrete receiver type.
///
/// Register once and reuse. Name lookup is O(n); indexed dispatch is O(1).
/// Only registered members are visible. The module attribute automates registration
/// and field-ordered construction; manual descriptors opt in to construction.
/// There is no inheritance, overload resolution, coercion, or field mutation.
pub struct TypeDescriptor {
    constructor: Option<fn(Vec<DynamicObject<'static>>) -> PixuiResult<DynamicObject<'static>>>,
    send_constructor: Option<SendConstructor>,
    kind: TypeKind,
    type_id: TypeId,
    type_name: &'static str,
    fields: Vec<Field>,
    methods: Vec<Method>,
}

impl TypeDescriptor {
    pub fn kind(&self) -> &TypeKind {
        &self.kind
    }
    pub fn is_sequence(&self) -> bool {
        matches!(self.kind, TypeKind::Sequence(_))
    }
    pub fn element_type(&self) -> Option<&'static TypeDescriptor> {
        match &self.kind {
            TypeKind::Sequence(sequence) => Some(sequence.element_type()),
            TypeKind::Struct => None,
        }
    }

    pub(super) fn sequence<T: ?Sized + 'static>(sequence: SequenceDescriptor) -> Self {
        Self {
            constructor: None,
            send_constructor: None,
            kind: TypeKind::Sequence(sequence),
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            fields: vec![],
            methods: vec![],
        }
    }

    pub(super) fn sequence_access(&self) -> PixuiResult<&crate::sequence::SequenceCallbacks> {
        match &self.kind {
            TypeKind::Sequence(sequence) => sequence
                .access
                .as_ref()
                .ok_or_else(|| pixui_error!("slice access requires sequence storage")),
            TypeKind::Struct => Err(pixui_error!("type `{}` is not a sequence", self.type_name)),
        }
    }

    /// Rust type name for diagnostics, not a stable identifier.
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub(super) fn check_receiver(&self, receiver: &dyn Any) -> PixuiResult<()> {
        if receiver.type_id() != self.type_id {
            pixui_bail!("receiver must have type `{}`", self.type_name);
        }
        Ok(())
    }

    /// Preserves registration order and rejects duplicate names within each
    /// namespace. A field and a method may share a name.
    pub fn new<T: Any>(fields: Vec<Field>, methods: Vec<Method>) -> PixuiResult<Self> {
        validate_names(fields.iter().map(|field| field.name), "field")?;
        validate_names(methods.iter().map(|method| method.name), "method")?;
        Ok(Self {
            constructor: None,
            send_constructor: None,
            kind: TypeKind::Struct,
            fields,
            methods,
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
        })
    }

    /// Registers a field-ordered constructor. The callback must return this type.
    pub fn with_constructor(
        mut self,
        constructor: fn(Vec<DynamicObject<'static>>) -> PixuiResult<DynamicObject<'static>>,
    ) -> Self {
        self.constructor = Some(constructor);
        self
    }

    pub fn is_constructible(&self) -> bool {
        self.constructor.is_some()
    }

    /// Registers an owned, sendable constructor. The callback must return this type.
    pub fn with_send_constructor(mut self, constructor: SendConstructor) -> Self {
        self.send_constructor = Some(constructor);
        self
    }

    pub fn is_send_constructible(&self) -> bool {
        self.send_constructor.is_some()
    }

    /// Constructs an owned sendable value from positional sendable fields.
    /// Count and result type are checked; generated callbacks check field types.
    /// No coercions, defaults, or runtime conversion from non-Send storage occur.
    pub fn construct_send(&self, fields: SendValues) -> PixuiResult<SendValue> {
        let constructor = self
            .send_constructor
            .ok_or_else(|| pixui_error!("type `{}` has no sendable constructor", self.type_name))?;
        if fields.len() != self.fields.len() {
            pixui_bail!(
                "type `{}` expects {} fields, got {}",
                self.type_name,
                self.fields.len(),
                fields.len()
            );
        }
        let result = constructor(fields)?;
        self.check_receiver(result.as_ref())?;
        Ok(result)
    }

    /// Consumes exactly one owned value per field, in descriptor order.
    /// No coercions or defaults are applied. Borrowed storage is rejected even
    /// when its lifetime is static. Errors consume and drop the supplied values.
    pub fn construct(
        &self,
        fields: Vec<DynamicObject<'static>>,
    ) -> PixuiResult<DynamicObject<'static>> {
        let constructor = self
            .constructor
            .ok_or_else(|| pixui_error!("type `{}` has no constructor", self.type_name))?;
        if fields.len() != self.fields.len() {
            pixui_bail!(
                "type `{}` expects {} fields, got {}",
                self.type_name,
                self.fields.len(),
                fields.len()
            );
        }
        for (index, value) in fields.iter().enumerate() {
            if !value.is_owned() {
                pixui_bail!(
                    "field {} `{}` requires an owned value",
                    index,
                    self.fields[index].name
                );
            }
        }
        let result = constructor(fields)?;
        if !result.is_owned() || result.descriptor().type_id() != self.type_id {
            pixui_bail!("constructor must return an owned `{}`", self.type_name);
        }
        Ok(result)
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn methods(&self) -> &[Method] {
        &self.methods
    }

    /// Looks up an exact, case-sensitive field name.
    pub fn field_index(&self, name: &str) -> PixuiResult<FieldIndex> {
        self.fields
            .iter()
            .position(|field| field.name == name)
            .map(FieldIndex)
            .ok_or_else(|| pixui_error!("unknown field `{name}`"))
    }

    /// Looks up an exact, case-sensitive method name.
    pub fn method_index(&self, name: &str) -> PixuiResult<MethodIndex> {
        self.methods
            .iter()
            .position(|method| method.name == name)
            .map(MethodIndex)
            .ok_or_else(|| pixui_error!("unknown method `{name}`"))
    }

    /// Reads by index, without name lookup, copying, or allocating a value.
    pub fn read<'a>(&self, receiver: &'a dyn Any, index: FieldIndex) -> PixuiResult<&'a dyn Any> {
        self.check_receiver(receiver)?;
        let field = self
            .fields
            .get(index.0)
            .ok_or_else(|| pixui_error!("invalid field index {}", index.0))?;
        (field.get)(receiver)
    }

    /// Resolves the name and delegates to [`Self::read`].
    pub fn read_object<'a>(
        &self,
        receiver: &'a dyn Any,
        index: FieldIndex,
    ) -> PixuiResult<DynamicObject<'a>> {
        self.check_receiver(receiver)?;
        let field = self
            .fields
            .get(index.0)
            .ok_or_else(|| pixui_error!("invalid field index {}", index.0))?;
        let get = field
            .get_object
            .as_ref()
            .ok_or_else(|| pixui_error!("field `{}` has no reflected object getter", field.name))?;
        get(receiver)
    }

    /// Resolves the name and reads a reflected object.
    pub fn read_object_named<'a>(
        &self,
        receiver: &'a dyn Any,
        name: &str,
    ) -> PixuiResult<DynamicObject<'a>> {
        self.read_object(receiver, self.field_index(name)?)
    }

    /// Resolves the name and delegates to [`Self::read`].
    pub fn read_named<'a>(&self, receiver: &'a dyn Any, name: &str) -> PixuiResult<&'a dyn Any> {
        self.read(receiver, self.field_index(name)?)
    }

    /// Invokes by index after validating the argument count.
    ///
    /// Adapter errors propagate unchanged. There is no rollback of mutations
    /// and adapter panics are not caught. Validate types before mutating.
    pub fn invoke(
        &self,
        receiver: &mut dyn Any,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.check_receiver(receiver)?;
        let method = self.checked_method(index, arguments)?;
        match &method.invocation {
            Invocation::Owned(invoke) => invoke(receiver, arguments),
            Invocation::SharedOwned(invoke) => invoke(receiver, arguments),
            _ => Err(pixui_error!(
                "method `{}` does not return an owned value",
                method.name
            )),
        }
    }

    fn checked_method(&self, index: MethodIndex, arguments: &[&dyn Any]) -> PixuiResult<&Method> {
        let method = self
            .methods
            .get(index.0)
            .ok_or_else(|| pixui_error!("invalid method index {}", index.0))?;
        if arguments.len() != method.arity {
            pixui_bail!(
                "method `{}` expects {} arguments, got {}",
                method.name,
                method.arity,
                arguments.len()
            );
        }
        Ok(method)
    }

    /// Calls an owned-returning method with a shared receiver.
    pub fn invoke_shared(
        &self,
        receiver: &dyn Any,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.check_receiver(receiver)?;
        let method = self.checked_method(index, arguments)?;
        match &method.invocation {
            Invocation::SharedOwned(invoke) => invoke(receiver, arguments),
            Invocation::Owned(_) => Err(pixui_error!(
                "method `{}` requires mutable receiver access",
                method.name
            )),
            _ => Err(pixui_error!(
                "method `{}` does not return an owned value",
                method.name
            )),
        }
    }

    /// Returns a reflected shared reference borrowed only from the receiver.
    pub fn invoke_ref<'a>(
        &self,
        receiver: &'a dyn Any,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'a>> {
        self.check_receiver(receiver)?;
        let method = self.checked_method(index, arguments)?;
        match &method.invocation {
            Invocation::Ref(invoke) => invoke(receiver, arguments),
            _ => Err(pixui_error!(
                "method `{}` does not return a shared reference",
                method.name
            )),
        }
    }

    /// Returns a reflected exclusive reference borrowed only from the receiver.
    pub fn invoke_mut<'a>(
        &self,
        receiver: &'a mut dyn Any,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'a>> {
        self.check_receiver(receiver)?;
        let method = self.checked_method(index, arguments)?;
        match &method.invocation {
            Invocation::Mut(invoke) => invoke(receiver, arguments),
            _ => Err(pixui_error!(
                "method `{}` does not return a mutable reference",
                method.name
            )),
        }
    }

    pub fn invoke_shared_named(
        &self,
        receiver: &dyn Any,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.invoke_shared(receiver, self.method_index(name)?, arguments)
    }

    pub fn invoke_ref_named<'a>(
        &self,
        receiver: &'a dyn Any,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'a>> {
        self.invoke_ref(receiver, self.method_index(name)?, arguments)
    }

    pub fn invoke_mut_named<'a>(
        &self,
        receiver: &'a mut dyn Any,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'a>> {
        self.invoke_mut(receiver, self.method_index(name)?, arguments)
    }

    /// Resolves the name and delegates to [`Self::invoke`].
    pub fn invoke_named(
        &self,
        receiver: &mut dyn Any,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.invoke(receiver, self.method_index(name)?, arguments)
    }
}

fn validate_names<'a>(names: impl Iterator<Item = &'a str>, kind: &str) -> PixuiResult<()> {
    let mut seen = std::collections::HashSet::new();
    for name in names {
        if name.is_empty() {
            pixui_bail!("{kind} name must not be empty");
        }
        if !seen.insert(name) {
            pixui_bail!("duplicate {kind} name `{name}`");
        }
    }
    Ok(())
}
