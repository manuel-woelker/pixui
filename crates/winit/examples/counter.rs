use pixui_base::result::PixuiResult;
use pixui_engine::components::label_painter::LabelPainter;
use pixui_engine::engine::Engine;
use pixui_winit::WinitAdapter;

fn main() -> PixuiResult<()> {
    let engine = Engine::new()?;
    let winit_adapter = WinitAdapter::new(&engine)?;

    winit_adapter.register_component_painter("label", LabelPainter);
    winit_adapter.create_window("label")?;
    Ok(())
}
