use anyhow::Result;
use egui::{style::Spacing, Color32, Margin, Stroke, Style, Visuals, Widget};
use layer_shell_wgpu_egui::{application::Msg, layer_shell::LayerShellOptions};
use sctk::shell::wlr_layer::KeyboardInteractivity;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let options = LayerShellOptions {
        width: 500,
        height: 500,
        ..Default::default()
    };

    // application state
    let mut name = "Alice".to_owned();
    let mut age = 26;

    let (msg, mut app) = layer_shell_wgpu_egui::run_layer_simple_pass(options, move |ctx| {
        let mut li = Visuals::dark();
        li.override_text_color = Some(Color32::WHITE.gamma_multiply(0.7));
        li.window_fill = Color32::BLACK.gamma_multiply(0.5);
        ctx.set_visuals(li);
        // egui::Frame::none().show(ctx, |ui| {});

        if true {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::none()
                        .fill(Color32::WHITE.gamma_multiply(0.05))
                        .inner_margin(Margin::same(15.)),
                )
                .show(ctx, |ui| {});
        }
    });

    std::thread::spawn(move || {
        use std::io::{self, Write};
        println!("Type 'h' to hide the window, 'unhide' to show it, or 'quit' to exit:");
        loop {
            print!("> ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                println!("Failed to read input");
                continue;
            }
            let cmd = input.trim();
            match cmd {
                "h" => {
                    if let Err(e) = msg.send(Msg::Hide(true)) {
                        println!("Failed to hide window");
                    } else {
                        println!("Window hidden.");
                    }
                }
                "s" => {
                    if let Err(e) = msg.send(Msg::Hide(false)) {
                        println!("Failed to show window");
                    } else {
                        println!("Window shown.");
                    }
                }
                "quit" => {
                    println!("Exiting...");
                    break;
                }
                _ => println!("Unknown command: {}", cmd),
            }
        }
    });

    app.run()?;

    Ok(())
}
