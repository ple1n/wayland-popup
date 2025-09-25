use anyhow::Result;
use egui::{style::Spacing, Color32, Margin, Stroke, Style, Visuals};
use layer_shell_wgpu_egui::{application::Msg, layer_shell::LayerShellOptions};
use sctk::shell::wlr_layer::KeyboardInteractivity;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let options = LayerShellOptions {
        width: 500,
        height: 300,
        anchor: None,
        keyboard_interactivity: Some(KeyboardInteractivity::OnDemand),
        ..Default::default()
    };

    // application state
    let mut name = "Alice".to_owned();
    let mut age = 26;

    let (msg, mut app) = layer_shell_wgpu_egui::run_layer_simple(options, move |ctx, sx| {
        let mut li = Visuals::dark();
        li.override_text_color = Some(Color32::WHITE.gamma_multiply(0.7));
        ctx.set_visuals(li);
        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::WHITE.gamma_multiply(0.1)).inner_margin(Margin::same(15.))).show(ctx, |ui| {
            ui.heading("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.");
            ui.horizontal(|ui| {
                let name_label = ui.label("Your name: ");
                ui.text_edit_singleline(&mut name)
                    .labelled_by(name_label.id);
            });
            ui.add(egui::Slider::new(&mut age, 0..=120).text("age"));
            if ui.button("Increment").clicked() {
                age += 1;
            }
            ui.label(format!("Hello '{name}', age {age}"));
        });
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
