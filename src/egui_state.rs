use egui::{epaint::ClippedShape, Context, FullOutput, TexturesDelta};
use egui_wgpu::{
    wgpu::{
        CommandEncoder, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
        RenderPassDescriptor, StoreOp, TextureFormat, TextureView,
    },
    Renderer, ScreenDescriptor,
};
use tracing::info;

// crates/egui-winit/src/lib.rs
pub struct State {
    context: egui::Context,
    egui_input: egui::RawInput,
    renderer: Renderer,
    start_time: std::time::Instant,

    /// track ime state
    has_sent_ime_enabled: bool,
}

impl State {
    pub fn ime_event_enable(&mut self) {
        if !self.has_sent_ime_enabled {
            info!("enable ime");
            self.egui_input
                .events
                .push(egui::Event::Ime(egui::ImeEvent::Enabled));
            self.has_sent_ime_enabled = true;
        }
    }
    pub fn ime_event_disable(&mut self) {
        self.egui_input
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Disabled));
        self.has_sent_ime_enabled = false;
    }

    pub fn new(
        context: egui::Context,
        device: &Device,
        output_color_format: TextureFormat,
        output_depth_format: Option<TextureFormat>,
        msaa_samples: u32,
    ) -> Self {
        let input = egui::RawInput {
            focused: true,
            viewport_id: egui::ViewportId::ROOT,
            ..Default::default()
        };

        let renderer = Renderer::new(
            device,
            output_color_format,
            output_depth_format,
            msaa_samples,
        );

        // input
        //     .viewports
        //     .entry(egui::ViewportId::ROOT)
        //     .or_default()
        //     .native_pixels_per_point = Some(1.0);

        Self {
            context,
            egui_input: input,
            renderer,
            start_time: std::time::Instant::now(),
            has_sent_ime_enabled: false,
        }
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        let screen_rect = egui::Rect {
            min: egui::Pos2 { x: 0f32, y: 0f32 },
            max: egui::Pos2 {
                x: width as f32,
                y: height as f32,
            },
        };
        self.egui_input.screen_rect = Some(screen_rect);
    }

    pub(crate) fn input(&mut self) -> &mut egui::RawInput {
        &mut self.egui_input
    }

    pub fn context(&self) -> &egui::Context {
        &self.context
    }

    pub fn modifiers(&self) -> egui::Modifiers {
        self.egui_input.modifiers
    }

    pub fn push_event(&mut self, event: egui::Event) {
        self.egui_input.events.push(event);
    }

    pub fn process_events(&mut self, run_ui: impl FnOnce(&Context)) -> FullOutput {
        // TODO: maybe we need to take input for a certain window / surface?
        self.egui_input.time = Some(self.start_time.elapsed().as_secs_f64());

        let raw_input = self.egui_input.take();
        /* if (&raw_input.events).len() > 0 {
            dbg!(&raw_input.events);
        } */
        self.context.run(raw_input, run_ui)
    }

    pub fn draw(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        window_surface_view: &TextureView,
        screen_descriptor: ScreenDescriptor,
        shapes: Vec<ClippedShape>,
        textures_delta: TexturesDelta,
    ) {
        //self.context.set_pixels_per_point(screen_descriptor.pixels_per_point);

        // iterate over viewport outputs
        /* for output in full_output.viewport_output.values() {
            dbg!(&output.repaint_delay);
        } */
        
        //dbg!(&full_output.);

        // TODO: implement platform output handling
        // this is for things like clipboard support
        //self.state.handle_platform_output(window, full_output.platform_output);

        let tris = self
            .context
            .tessellate(shapes, self.context.pixels_per_point());
        for (id, image_delta) in &textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }
        self.renderer
            .update_buffers(device, queue, encoder, &tris, &screen_descriptor);
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("egui main render pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: window_surface_view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(egui_wgpu::wgpu::Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        self.renderer.render(&mut rpass, &tris, &screen_descriptor);
        drop(rpass);
        for x in &textures_delta.free {
            self.renderer.free_texture(x)
        }
    }
}
