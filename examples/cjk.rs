use anyhow::Result;
use egui::{
    epaint::text::FontInsert, style::Spacing, Color32, FontData, FontFamily, Margin, Stroke, Style,
    Visuals,
};
use layer_shell_wgpu_egui::{
    application::Msg, errors::wrap_noncritical_sync, layer_shell::LayerShellOptions, App,
    AppCreator,
};
use sctk::shell::wlr_layer::KeyboardInteractivity;
use tracing::level_filters::LevelFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .init();

    let options = LayerShellOptions {
        width: 500,
        height: 300,
        anchor: None,
        keyboard_interactivity: Some(KeyboardInteractivity::OnDemand),
        ..Default::default()
    };

    // application state

    #[derive(Default)]
    struct CjkApp {
        gamma: f32,
    }

    impl App for CjkApp {
        fn update(&mut self, ctx: &egui::Context) {
            // performance on par with offical egui impl rn.
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(Color32::WHITE.gamma_multiply(self.gamma))
                        .inner_margin(Margin::same(15)),
                )
                .show(ctx, |ui| {
                    ui.heading("Wayland popup app framework");
                    ui.add(egui::Slider::new(&mut self.gamma, 0.0..=0.5).text("gamma"));
                });
        }
    }

    let (msg, mut app) = layer_shell_wgpu_egui::run_layer(
        options,
        Box::new(|ctx, sx| {
            let mut li = Visuals::dark();
            li.override_text_color = Some(Color32::WHITE.gamma_multiply(0.7));
            ctx.set_visuals(li);
            egui_chinese_font::setup_chinese_fonts(ctx).unwrap();
            Ok(Box::new(CjkApp::default()))
        }),
    );

    msg.send(Msg::Passthrough(false))?;

    std::thread::spawn(move || {
        use std::io::{self, Write};
        println!("Type 'h' to hide the window, 's' to show it");
        loop {
            print!("> ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                println!("Failed to read input");
                continue;
            }
            let cmd = input.trim();
            wrap_noncritical_sync(|| {
                match cmd {
                    "h" => {
                        msg.send(Msg::Hide(true))?;
                    }
                    "s" => {
                        msg.send(Msg::Hide(false))?;
                        msg.send(Msg::Passthrough(false))?;
                    }
                    "p" => {
                        msg.send(Msg::Passthrough(true))?;
                    },
                    _ => println!("Unknown command: {}", cmd),
                }
                anyhow::Ok(())
            });
        }
    });

    app.run()?;

    Ok(())
}
