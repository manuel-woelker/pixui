//! Repeat worker renders without a native window for reproducible CPU profiling.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::app::Application,
    ui::{input::UiCommand, performance::WorkerTimings, presentation::PresentationSettings},
};
use pixui_example_todo::{gui_ui, orbiting_comets, todo};
use std::time::Duration;

fn main() -> PixuiResult<()> {
    let mut frames = 1000;
    let mut todos = 2;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| pixui_error!("{arg} requires a value"))?;
        let value = value
            .parse::<usize>()
            .map_err(|_| pixui_error!("{arg} requires a nonnegative integer"))?;
        match arg.as_str() {
            "--frames" => frames = value,
            "--todos" => todos = value,
            _ => return Err(pixui_error!("unknown option `{arg}`")),
        }
    }
    if frames == 0 {
        return Err(pixui_error!("--frames must be positive"));
    }
    let app = Application::new();
    todo::create_slice(&app)?;
    let actions = todo::actions::TodoActions::bind(&app)?;
    for index in 0..todos {
        actions.add_todo(format!("Todo {index}: profile layout and text measurement"))?;
    }
    let components = app.register_standard_components()?;
    app.register_standard_painters()?;
    let definition = app.register_ui(gui_ui::definition(
        &app,
        components,
        orbiting_comets::register(&app)?,
    )?)?;
    let (instance, outputs) = app.create_ui(
        definition,
        PresentationSettings {
            timestamp_us: Some(0),
            ..Default::default()
        },
    )?;
    outputs
        .recv_timeout(Duration::from_secs(10))
        .map_err(|error| pixui_error!("initial output: {error}"))?;
    let mut samples = Vec::with_capacity(frames);
    // Exclude first-use font/image setup and warm the worker caches.
    for index in 0..frames + 20 {
        app.ui_command(UiCommand::Redraw { instance })?;
        let output = outputs
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| pixui_error!("render output: {error}"))?;
        if index >= 20 {
            samples.push(output.timings);
        }
    }
    println!(
        "{} build; {frames} warmed frames; {todos} todos; viewport 800x600; frozen animation; no native renderer",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    report("Preparation", &samples, |sample| sample.preparation);
    report("Tree construction", &samples, |sample| {
        sample.tree_construction
    });
    report("Layout incl. measurement/extraction", &samples, |sample| {
        sample.layout
    });
    report("Painting", &samples, |sample| sample.painting);
    report("Text finalization", &samples, |sample| sample.text);
    println!(
        "Measurement calls/frame: {:.1}",
        samples
            .iter()
            .map(|sample| sample.measurements as f64)
            .sum::<f64>()
            / samples.len() as f64
    );
    Ok(())
}
fn report(label: &str, samples: &[WorkerTimings], value: fn(&WorkerTimings) -> Duration) {
    let mut times: Vec<_> = samples
        .iter()
        .map(|sample| value(sample).as_secs_f64() * 1000.0)
        .collect();
    times.sort_by(f64::total_cmp);
    println!(
        "{label:<36} median {:>8.3} ms   p95 {:>8.3} ms",
        times[times.len() / 2],
        times[((times.len() - 1) * 95) / 100]
    );
}
