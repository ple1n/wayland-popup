use std::process;

use anyhow::Result;
use egui::{
    epaint::text::FontInsert, style::Spacing, Color32, FontData, FontFamily, Margin, Stroke, Style,
    Visuals,
};
use wpopup::{
    application::Msg, errors::wrap_noncritical_sync, layer_shell::LayerShellOptions, App,
    AppCreator,
};
use sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity};
use tokio::sync::watch;
use tracing::{info, level_filters::LevelFilter};
use wayland_clipboard_listener::WlListenType;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .init();

    let options = LayerShellOptions {
        width: 400,
        height: 800,
        anchor: Some(Anchor::LEFT),
        margin: (50, 50, 50, 50),
        keyboard_interactivity: Some(KeyboardInteractivity::OnDemand),
        ..Default::default()
    };

    let (p_sx, p_rx) = watch::channel("selection".to_owned());

    // application state
    struct CjkApp {
        gamma: f32,
        select: watch::Receiver<String>,
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
                    ui.label(egui::RichText::new(
                        self.select.borrow_and_update().to_owned(),
                    ));
                    ui.add_space(20.);
                    if ui.button("exit").clicked() {
                        process::exit(0);
                    }
                });
        }
    }

    let (msg, mut app) = wpopup::run_layer(
        options,
        Box::new(|ctx, sx| {
            let mut li = Visuals::dark();
            li.override_text_color = Some(Color32::WHITE.gamma_multiply(0.8));
            ctx.set_visuals(li);
            egui_chinese_font::setup_chinese_fonts(ctx).unwrap();
            Ok(Box::new(CjkApp {
                gamma: 0.04,
                select: p_rx,
            }))
        }),
    );
    msg.send(Msg::Passthrough(false))?;
    let msg2  = msg.clone();

    std::thread::spawn(move || {
        wrap_noncritical_sync(|| {
            let mut lis = wayland_clipboard_listener::WlClipboardPasteStream::init(
                WlListenType::ListenOnSelect,
            )?;
            for ctx in lis.paste_stream().flatten() {
                let stx = String::from_utf8(ctx.context.context);
                if let Ok(stx) = stx {
                    info!("select {:?}", &stx);
                    p_sx.send(stx)?;
                    msg2.send(Msg::Repaint)?;
                }
            }
            anyhow::Ok(())
        });
    });

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
                    }
                    _ => println!("Unknown command: {}", cmd),
                }
                anyhow::Ok(())
            });
        }
    });

    app.run_forever()?;

    Ok(())
}
