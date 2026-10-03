use std::any::{Any, TypeId, type_name};

use pixui_base::{PixuiResult, pixui_bail, pixui_error};

use crate::{Field, FieldIndex, Method, MethodIndex};

/// Immutable registration for a concrete receiver type.
///
/// Register once and reuse. Name lookup is O(n); indexed dispatch is O(1).
/// Only registered members are visible. The module attribute automates registration.
/// There is no inheritance, overload resolution, coercion, or field mutation.
pub struct TypeDescriptor {
    type_id: TypeId,
    type_name: &'static str,
    fields: Vec<Field>,
    methods: Vec<Method>,
}

impl TypeDescriptor {
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
            fields,
            methods,
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
        })
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
        (method.invoke)(receiver, arguments)
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
