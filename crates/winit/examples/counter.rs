use facet::Facet;
use pixui_base::result::PixuiResult;
use pixui_engine::engine::Engine;

#[derive(Debug, Facet)]
struct Counter {
    count: u32,
}

fn main() -> PixuiResult<()> {
    let engine = Engine::new()?;

    let count_ref = engine.run_application(|application| {
        let entity_key = application.add_entity(Counter { count: 0 })?;
        Ok(entity_key)
    })?;

    println!("counter entity count: {count_ref:?}");
    Ok(())
}
