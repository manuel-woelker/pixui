//! Event interfaces are typed at emission and reflected at language boundaries.
use pixui_base::PixuiResult;
use pixui_engine::events::interface::{EventInterface, EventSink};
use pixui_engine_macros::component_events;
use pixui_reflect::DynamicObject;

#[component_events]
trait EditorEvents {
    fn change(&self, content: String, accepted: bool) -> PixuiResult<()>;
    fn submit(&self) -> PixuiResult<()>;
}
#[component_events]
trait EmptyEvents {}

#[test]
fn proxy_emits_named_owned_payloads_in_method_order_through_shared_references() {
    let descriptors = <dyn EditorEvents>::descriptors();
    assert_eq!(descriptors[0].name, "change");
    assert_eq!(descriptors[0].payload.fields()[0].name, "content");
    assert_eq!(descriptors[0].payload.fields()[1].name, "accepted");
    let sink = EventSink::new(descriptors, vec![true, true]).unwrap();
    <dyn EditorEvents>::with_proxy(&sink, |events| {
        let shared = events;
        shared.change("héllo".into(), true).unwrap();
        events.submit().unwrap();
        events.change("second".into(), false).unwrap();
    });
    let pending = sink.into_pending();
    assert_eq!(
        pending.iter().map(|event| event.index).collect::<Vec<_>>(),
        [0, 1, 0]
    );
    assert_eq!(
        pending[0]
            .payload
            .read_named("content")
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "héllo"
    );
    assert_eq!(
        pending[0]
            .payload
            .read_named("accepted")
            .unwrap()
            .downcast_ref::<bool>(),
        Some(&true)
    );
    assert!(pending[1].payload.descriptor().fields().is_empty());
}
#[test]
fn unbound_methods_and_empty_interfaces_are_noops_and_invalid_payloads_fail() {
    let sink = EventSink::new(<dyn EditorEvents>::descriptors(), vec![false, false]).unwrap();
    <dyn EditorEvents>::with_proxy(&sink, |events| {
        events.change("ignored".into(), false).unwrap();
        events.submit().unwrap();
    });
    assert!(sink.emit(99, DynamicObject::from_reflect(true)).is_err());
    assert!(sink.emit(0, DynamicObject::from_reflect(true)).is_err());
    assert!(sink.into_pending().is_empty());
    let sink = EventSink::new(<dyn EmptyEvents>::descriptors(), vec![]).unwrap();
    <dyn EmptyEvents>::with_proxy(&sink, |_| {});
    assert!(sink.into_pending().is_empty());
    assert!(EventSink::new(<dyn EditorEvents>::descriptors(), vec![]).is_err());
}
