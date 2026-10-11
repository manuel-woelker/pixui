//! Profile the full loop-expanded layout page without native windows or a renderer.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::ui::{
    geometry::Size, input::UiCommand, performance::WorkerTimings,
    presentation::PresentationSettings,
};
use pixui_example_showcase::{model::actions::ShowcaseActions, setup};
use std::time::Duration;

fn main() -> PixuiResult<()> {
    let mut frames = 500;
    let mut width = 800;
    let mut height = 600;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| pixui_error!("{arg} requires a value"))?
            .parse::<usize>()
            .map_err(|_| pixui_error!("{arg} requires a positive integer"))?;
        match arg.as_str() {
            "--frames" => frames = value,
            "--width" => width = value,
            "--height" => height = value,
            _ => return Err(pixui_error!("unknown option `{arg}`")),
        }
    }
    if frames == 0 || frames > 100_000 || width == 0 || height == 0 {
        return Err(pixui_error!(
            "frames must be 1–100000 and viewport dimensions positive"
        ));
    }
    let (app, definition) = setup::register()?;
    ShowcaseActions::bind(&app)?.select(6)?;
    let (instance, outputs) = app.create_ui(
        definition,
        PresentationSettings {
            viewport: Size {
                width: width as f32,
                height: height as f32,
            },
            timestamp_us: Some(0),
            ..Default::default()
        },
    )?;
    let initial = outputs
        .recv_timeout(Duration::from_secs(10))
        .map_err(|error| pixui_error!("initial output: {error}"))?
        .timings;
    let mut samples = Vec::with_capacity(frames);
    for index in 0..frames + 20 {
        app.ui_command(UiCommand::Redraw { instance })?;
        let output = outputs
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| pixui_error!("render output: {error}"))?;
        if index >= 20 {
            samples.push(output.timings);
        }
    }
    let (components, containers) = app.inspect(move |app| {
        let layout = app.uis().instance(instance)?.layout();
        Ok((layout.component_bounds.len(), layout.container_bounds.len()))
    })?;
    println!(
        "{} build; {frames} warmed frames; {width}x{height}; {components} components; {containers} containers; no native renderer",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    report("Preparation", &samples, |sample| sample.preparation);
    println!(
        "Initial layout: {:.3} ms; {} measurements",
        initial.layout.as_secs_f64() * 1000.0,
        initial.measurements
    );
    report("Tree construction", &samples, |sample| {
        sample.tree_construction
    });
    report("Layout incl. measurement/extraction", &samples, |sample| {
        sample.layout
    });
    report("Painting", &samples, |sample| sample.painting);
    report("Text finalization", &samples, |sample| sample.text);
    report("Worker stage total", &samples, |sample| {
        sample.preparation
            + sample.tree_construction
            + sample.layout
            + sample.painting
            + sample.text
    });
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
        times[(times.len() - 1) * 95 / 100]
    );
}
