//! Setup-time loading and shared image ownership through a complete todo UI.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    ui::{
        display_list::DrawCommand,
        image::ImagePixels,
        presentation::{PresentationSettings, Theme},
        resource::Resource,
    },
};
use pixui_example_todo::{gui_ui, logo, orbiting_comets, todo};
use std::time::Duration;

#[test]
fn logo_is_loaded_once_shared_across_windows_and_fits_its_component() -> PixuiResult<()> {
    let app = Application::new();
    todo::create_slice(&app)?;
    let components = app.register_standard_components()?;
    app.register_standard_painters()?;
    let comets = orbiting_comets::register(&app)?;
    let definition = app.register_ui(gui_ui::definition(&app, components, comets)?)?;
    let frozen = PresentationSettings {
        timestamp_us: Some(0),
        ..Default::default()
    };
    let (_, light_outputs) = app.create_ui(definition, frozen.clone())?;
    let (_, dark) = app.create_ui(
        definition,
        PresentationSettings {
            theme: Theme::Dark,
            ..frozen
        },
    )?;
    let light = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
    let dark = dark.recv_timeout(Duration::from_secs(2)).unwrap();
    let logo = &light.display_list.images[1];
    assert!(Resource::ptr_eq(logo, &dark.display_list.images[1]));
    assert!(
        matches!(logo.pixels(), ImagePixels::Rgba { pixels } if pixels.iter().any(|p| p.3 == 0) && pixels.iter().any(|p| p.3 > 0))
    );
    for output in [&light, &dark] {
        let destination = output
            .display_list
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::DrawImage { image, destination }
                    if Resource::ptr_eq(&output.display_list.images[*image], logo) =>
                {
                    Some(*destination)
                }
                _ => None,
            })
            .unwrap();
        assert!(destination.width > 0.0 && destination.height > 0.0);
        assert!(
            (destination.width / destination.height - logo.width() as f32 / logo.height() as f32)
                .abs()
                < 0.0001
        );
    }
    let actions = todo::actions::TodoActions::bind(&app)?;
    actions.add_todo("Keep the loaded logo")?;
    let updated = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(Resource::ptr_eq(logo, &updated.display_list.images[1]));
    logo::configure_resources(&app, None)?;
    let replaced = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!Resource::ptr_eq(logo, &replaced.display_list.images[1]));
    assert_eq!(logo.pixels(), replaced.display_list.images[1].pixels());
    Ok(())
}
#[test]
fn invalid_override_is_reported_instead_of_using_default_logo() {
    let root = std::env::temp_dir().join(format!("pixui-logo-override-{}", std::process::id()));
    std::fs::create_dir_all(root.join("images")).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::write(root.join("images/pixui-logo.png"), b"broken override").unwrap();
    let app = Application::new();
    logo::configure_resources(&app, Some(&root)).unwrap();
    let error = app
        .inspect(|app| {
            app.load_image(&pixui_engine::resources::path::ResourcePath::new(
                "images/pixui-logo.png",
            )?)
        })
        .unwrap_err();
    assert!(format!("{error:?}").contains("images/pixui-logo.png"));
    // A valid override and an absent override both complete setup normally.
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/images/pixui-logo.png"
        ),
        root.join("images/pixui-logo.png"),
    )
    .unwrap();
    logo::configure_resources(&app, Some(&root)).unwrap();
    assert!(
        app.inspect(
            |app| app.load_image(&pixui_engine::resources::path::ResourcePath::new(
                "images/pixui-logo.png"
            )?)
        )
        .is_ok()
    );
    std::fs::remove_file(root.join("images/pixui-logo.png")).unwrap();
    logo::configure_resources(&app, Some(&root)).unwrap();
    assert!(
        app.inspect(
            |app| app.load_image(&pixui_engine::resources::path::ResourcePath::new(
                "images/pixui-logo.png"
            )?)
        )
        .is_ok()
    );
}
