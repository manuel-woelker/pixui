use facet::Facet;
use pixui_base::result::PixuiResult;
use pixui_engine::engine::Engine;

#[derive(Debug, Facet)]
struct Counter {
    count: u32,
}

#[derive(Debug)]
struct IncrementCount;

fn main() -> PixuiResult<()> {
    let engine = Engine::new()?;

    let count_ref = engine.run_application(|application| {
        let entity_key = application.add_entity(Counter { count: 0 })?;
        Ok(entity_key)
    })?;
    engine.on_event::<IncrementCount>(move |context| {
        let counter = context.get_entity_mut(count_ref)?;
        counter.count = counter.count.saturating_add(1);
        Ok(())
    })?;
    engine.submit_event(IncrementCount)?;
    let count = engine.run_application(move |application| {
        Ok(application.entity_store().get_entity(count_ref)?.count)
    })?;

    println!("counter entity: {count_ref:?}, count: {count}");
    Ok(())
}
