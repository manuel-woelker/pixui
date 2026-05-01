use facet::Facet;
use pixui_base::result::PixuiResult;
use pixui_engine::app::Application;
use pixui_engine::engine::Engine;
use pixui_engine::engine_event_context::EngineEventContext;
use pixui_engine::engine_event_handler::EngineEventHandler;
use pixui_engine::entity::store::TypedEntityKey;

#[derive(Debug, Facet)]
struct Counter {
    count: u32,
}

#[derive(Debug)]
struct IncrementCount;

struct IncrementCountHandler {
    counter_key: TypedEntityKey<Counter>,
}

impl EngineEventHandler for IncrementCountHandler {
    type Event = IncrementCount;

    fn handle_event(
        &mut self,
        application: &mut Application,
        _context: &mut EngineEventContext<Self::Event>,
    ) -> PixuiResult<()> {
        let counter = application
            .entity_store_mut()
            .get_entity_mut(self.counter_key)?;
        counter.count = counter.count.saturating_add(1);
        Ok(())
    }
}

fn main() -> PixuiResult<()> {
    let engine = Engine::new()?;

    let count_ref = engine.run_application(|application| {
        let entity_key = application.add_entity(Counter { count: 0 })?;
        Ok(entity_key)
    })?;
    engine.register_event_handler(IncrementCountHandler {
        counter_key: count_ref,
    })?;
    engine.submit_event(IncrementCount)?;
    let count = engine.run_application(move |application| {
        Ok(application.entity_store().get_entity(count_ref)?.count)
    })?;

    println!("counter entity: {count_ref:?}, count: {count}");
    Ok(())
}
