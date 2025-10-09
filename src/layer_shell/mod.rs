mod keyboard_handler;
mod pointer_handler;

use std::{
    io::PipeReader, sync::{Arc, RwLock}, time::{Duration, Instant}, u32
};

use dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size};
use egui::{
    ahash::{AHashMap, HashMap},
    PlatformOutput, ViewportCommand,
};
use egui_wgpu::ScreenDescriptor;
use keyboard_handler::handle_key_press;
pub use sctk::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::LoopHandle,
        calloop_wayland_source::WaylandSource,
        protocols::{
            ext::background_effect::v1::client::{
                ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
                ext_background_effect_surface_v1,
            },
            wp::text_input::zv3::client::{
                zwp_text_input_manager_v3::ZwpTextInputManagerV3, zwp_text_input_v3::ZwpTextInputV3,
            },
        },
        protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        xdg::window::Window,
        WaylandSurface,
    },
};

use sctk::{
    self,
    reexports::protocols_wlr::data_control::v1::client::{
        zwlr_data_control_device_v1, zwlr_data_control_manager_v1,
    },
};
use tokio::sync::mpsc;
use tracing::{info, warn};
use wayland_backend::client::ObjectId;
use wayland_client::{
    delegate_dispatch, delegate_noop,
    globals::registry_queue_init,
    protocol::{
        wl_keyboard::WlKeyboard, wl_output, wl_pointer::WlPointer, wl_region::WlRegion, wl_seat,
        wl_surface,
    },
    Connection, Proxy, QueueHandle,
};
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur::OrgKdeKwinBlur;
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur_manager::OrgKdeKwinBlurManager;

use crate::{
    application::WPEvent,
    egui_state::{self},
    layer_shell::cliphandler::WlListenType,
    text_input::{
        ImeCapabilities, ImeEnableRequest, ImeHint, ImePurpose, ImeRequest, ImeRequestData,
        ImeSurroundingText, TextInputClientState, TextInputData, TextInputState, ZwpTextInputV3Ext,
    },
    wgpu_state::WgpuState,
    App,
};

#[derive(Default)]
pub struct LayerShellOptions {
    pub layer: Option<Layer>,
    pub namespace: String,
    pub width: u32,
    pub height: u32,
    pub anchor: Option<Anchor>,
    pub keyboard_interactivity: Option<KeyboardInteractivity>,
    pub margin: (i32, i32, i32, i32),
}

pub struct WgpuLayerShellState {
    //event_loop: Arc<EventLoop<'static, Self>>,
    loop_handle: LoopHandle<'static, Self>,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    pub(crate) queue_handle: Arc<QueueHandle<Self>>,

    pub(crate) layer: LayerSurface,
    pub current_layer: Layer,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,

    pub(crate) has_frame_callback: bool,
    is_configured: bool,

    pub(crate) exit: bool,

    pub(crate) wgpu_state: WgpuState,
    pub egui_state: egui_state::State,
    pub(crate) draw_request: Arc<RwLock<Option<Instant>>>,

    /// The input method properties provided by the application to the IME.
    ///
    /// This state is cached here so that the window can automatically send the state to the IME as
    /// soon as it becomes available without application involvement.
    pub text_input_state: Option<TextInputClientState>,
    pub window_text_input_state: Option<TextInputState>,
    /// The text inputs observed on the window.
    pub text_inputs: Vec<ZwpTextInputV3>,
    pub seat_map: AHashMap<ObjectId, PerSeat>,

    /// The current IME purpose.
    pub ime_purpose: ImePurpose,

    /// Whether the IME input is allowed for that window.
    ime_allowed: bool,
    passthrough: bool,

    compositor: CompositorState,
    layer_opts: LayerShellOptions,

    listentype: WlListenType,
    seat: Option<wl_seat::WlSeat>,
    seat_name: Option<String>,
    data_manager: Option<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1>,
    data_device: Option<zwlr_data_control_device_v1::ZwlrDataControlDeviceV1>,
    mime_types: Vec<String>,
    set_priority: Option<Vec<String>>,
    pipereader: Option<PipeReader>,
    current_type: Option<String>,
    copy_data: Option<Vec<u8>>,
    copy_cancelled: bool,

    pub ev: mpsc::UnboundedSender<WPEvent>,
}

pub mod cliphandler;

#[derive(Default)]
pub struct PerSeat {
    pub text_input: Option<Arc<ZwpTextInputV3>>,
}

delegate_noop!(WgpuLayerShellState: ignore ExtBackgroundEffectManagerV1);
delegate_noop!(WgpuLayerShellState: ignore OrgKdeKwinBlurManager);
delegate_noop!(WgpuLayerShellState: ignore OrgKdeKwinBlur);
delegate_noop!(WgpuLayerShellState: ignore WlRegion);

/// Calculate the `pixels_per_point` for a given window, given the current egui zoom factor
pub fn pixels_per_point(egui_ctx: &egui::Context, scale: f32) -> f32 {
    let native_pixels_per_point = scale;
    let egui_zoom_factor = egui_ctx.zoom_factor();
    egui_zoom_factor * native_pixels_per_point
}

impl WgpuLayerShellState {
    /// Whether the IME is allowed.
    #[inline]
    pub fn ime_allowed(&self) -> bool {
        self.ime_allowed
    }

    pub fn set_ime_purpose(&mut self, purpose: ImePurpose) {
        self.ime_purpose = purpose;

        for text_input in &self.text_inputs {
            text_input.set_content_type_by_purpose(purpose);
            text_input.commit();
        }
    }

    pub fn set_ime_cursor_area(&self, position: Position, size: Size) {
        if self.ime_allowed() {
            let scale_factor = self.scale_factor();
            let position = position.to_logical(scale_factor);
            let size = size.to_logical(scale_factor);
            self.set_ime_cursor_area_inner(position, size);
        }
    }

    /// Set the IME position.
    pub fn set_ime_cursor_area_inner(
        &self,
        position: LogicalPosition<u32>,
        size: LogicalSize<u32>,
    ) {
        // FIXME: This won't fly unless user will have a way to request IME window per seat, since
        // the ime windows will be overlapping, but winit doesn't expose API to specify for
        // which seat we're setting IME position.
        let (x, y) = (position.x as i32, position.y as i32);
        let (width, height) = (size.width as i32, size.height as i32);
        for text_input in self.text_inputs.iter() {
            text_input.set_cursor_rectangle(x, y, width, height);
            text_input.commit();
        }
    }

    pub fn handle_platform(&mut self, platform_output: egui::PlatformOutput) {
        let egui::PlatformOutput {
            commands,
            cursor_icon,
            events: _,                    // handled elsewhere
            mutable_text_under_cursor: _, // only used in eframe web
            ime,
            #[cfg(feature = "accesskit")]
            accesskit_update,
            num_completed_passes: _,    // `egui::Context::run` handles this
            request_discard_reasons: _, // `egui::Context::run` handles this
            ..
        } = platform_output;

        if let Some(ime) = ime {
            self.set_ime_allowed(true);

            let pixels_per_point =
                pixels_per_point(self.egui_state.context(), self.scale_factor() as f32);
            let ime_rect_px = pixels_per_point * ime.rect;
            if self.egui_state.ime_rect_px != Some(ime_rect_px)
                || self.egui_state.context().input(|i| !i.events.is_empty())
            {
                self.egui_state.ime_rect_px = Some(ime_rect_px);
                self.set_ime_cursor_area(
                    dpi::PhysicalPosition {
                        x: ime_rect_px.min.x,
                        y: ime_rect_px.min.y,
                    }
                    .into(),
                    dpi::PhysicalSize {
                        width: ime_rect_px.width(),
                        height: ime_rect_px.height(),
                    }
                    .into(),
                );
            }
        } else {
            self.egui_state.ime_rect_px = None;
        }
    }

    /// Returns `true` if the requested state was applied.
    pub fn set_ime_allowed(&mut self, allowed: bool) -> bool {
        if self.ime_allowed == allowed {
            return false;
        }

        info!("set ime {}", allowed);
        self.ime_allowed = allowed;

        let mut applied = false;
        for text_input in &self.text_inputs {
            applied = true;
            if allowed {
                text_input.enable();
                text_input.set_content_type_by_purpose(self.ime_purpose);
            } else {
                text_input.disable();
            }
            text_input.commit();
        }

        applied
    }

    pub fn set_passthrough(&mut self, pass: bool) {
        if pass {
            let region = self
                .compositor
                .wl_compositor()
                .create_region(&self.queue_handle, ());
            self.layer.set_input_region(Some(&region));
        } else {
            self.layer.set_input_region(None);
        }
        self.passthrough = pass;
    }

    pub fn set_layer_opts(&mut self) {
        let options = &self.layer_opts;
        let layer_surface = &self.layer;
        if let Some(anchor) = options.anchor {
            layer_surface.set_anchor(anchor);
        }
        if let Some(keyboard_interactivity) = options.keyboard_interactivity {
            layer_surface.set_keyboard_interactivity(keyboard_interactivity);
        }
        layer_surface.set_size(options.width, options.height);
        layer_surface.set_opaque_region(None);
        layer_surface.set_margin(
            options.margin.0,
            options.margin.1,
            options.margin.2,
            options.margin.3,
        );
        layer_surface.commit();
    }

    pub(crate) fn new(
        loop_handle: LoopHandle<'static, Self>,
        options: LayerShellOptions,
        ev: mpsc::UnboundedSender<WPEvent>,
    ) -> Self {
        let connection = Connection::connect_to_env().unwrap();
        let (global_list, event_queue) = registry_queue_init(&connection).unwrap();
        let queue_handle: Arc<QueueHandle<WgpuLayerShellState>> = Arc::new(event_queue.handle());
        let globals = &global_list;
        // global_list
        //     .bind::<ExtBackgroundEffectManagerV1, _, _>(queue_handle.as_ref(), 0..=1, ())
        //     .unwrap();
        WaylandSource::new(connection.clone(), event_queue)
            .insert(loop_handle.clone())
            .unwrap();

        let display = connection.display();
        display.get_registry(&queue_handle, ());
        let compositor_state = CompositorState::bind(&global_list, &queue_handle)
            .expect("wl_compositor not available");
        let wl_surface = compositor_state.create_surface(&queue_handle);

        let kdeblur = global_list
            .bind::<OrgKdeKwinBlurManager, _, _>(queue_handle.as_ref(), 0..=1, ())
            .unwrap();

        let layer_shell =
            LayerShell::bind(&global_list, &queue_handle).expect("layer shell not available");
        let layer = options.layer.unwrap_or(Layer::Top);
        let layer_surface = layer_shell.create_layer_surface(
            &queue_handle,
            wl_surface,
            layer,
            Some(options.namespace.clone()),
            None,
        );

        if let Some(anchor) = options.anchor {
            layer_surface.set_anchor(anchor);
        }
        if let Some(keyboard_interactivity) = options.keyboard_interactivity {
            layer_surface.set_keyboard_interactivity(keyboard_interactivity);
        }
        layer_surface.set_size(options.width, options.height);
        layer_surface.set_opaque_region(None);
        layer_surface.set_margin(
            options.margin.0,
            options.margin.1,
            options.margin.2,
            options.margin.3,
        );
        layer_surface.commit();

        let seat_state = SeatState::new(globals, &queue_handle);

        let mut seats = AHashMap::default();
        for seat in seat_state.seats() {
            seats.insert(seat.id(), PerSeat::default());
        }

        let region = compositor_state
            .wl_compositor()
            .create_region(&queue_handle, ());
        let blur: OrgKdeKwinBlur = kdeblur.create(layer_surface.wl_surface(), &queue_handle, ());
        blur.set_region(Some(&region));
        blur.commit();

        let wgpu_state = WgpuState::new(&connection.backend(), layer_surface.wl_surface())
            .expect("Could not create wgpu state");
        let window_text_input_state = TextInputState::new(&global_list, &queue_handle).ok();

        let egui_context = egui::Context::default();
        let draw_request = Arc::new(RwLock::new(None));

        egui_context.set_request_repaint_callback({
            let draw_request = Arc::clone(&draw_request);
            move |info| {
                let mut draw_request = draw_request.write().unwrap();
                *draw_request = Some(Instant::now() + info.delay);
            }
        });

        let egui_state = egui_state::State::new(
            egui_context,
            &wgpu_state.device,
            wgpu_state.surface_configuration.format,
            None,
            1,
        );
        println!(
            "window_text_input_state {}",
            window_text_input_state.is_some()
        );
        WgpuLayerShellState {
            loop_handle: loop_handle.clone(),
            registry_state: RegistryState::new(&global_list),
            seat_state,
            output_state: OutputState::new(&global_list, &queue_handle),

            exit: false,
            layer: layer_surface,
            current_layer: layer,
            pointer: None,
            keyboard: None,

            has_frame_callback: false,
            is_configured: false,

            window_text_input_state,
            text_input_state: None,
            queue_handle,

            egui_state,
            wgpu_state,
            draw_request,

            text_inputs: vec![],
            seat_map: seats,
            ime_purpose: ImePurpose::Normal,
            ime_allowed: true,
            compositor: compositor_state,
            passthrough: false,
            layer_opts: options,

            listentype: WlListenType::ListenOnSelect,
            seat: None,
            seat_name: None,
            data_manager: None,
            data_device: None,
            mime_types: Vec::new(),
            set_priority: None,
            pipereader: None,
            current_type: None,
            copy_data: None,
            copy_cancelled: false,
            ev,
        }
    }

    pub fn set_margin(&mut self, margin: (i32, i32, i32, i32)) {
        self.layer
            .set_margin(margin.0, margin.1, margin.2, margin.3);
        self.layer.commit();
    }

    //fn request_redraw(&self, )

    pub(crate) fn should_draw(&mut self) -> bool {
        if !self.has_frame_callback {
            return false;
        }

        if !self.egui_state.input().events.is_empty() {
            return true;
        }

        match *self.draw_request.read().unwrap() {
            Some(time) => time <= Instant::now(),
            None => false,
        }
    }

    pub(crate) fn get_timeout(&self) -> Option<Duration> {
        match *self.draw_request.read().unwrap() {
            Some(instant) => {
                if self.has_frame_callback {
                    Some(instant.duration_since(Instant::now()))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub(crate) fn draw(&mut self, application: &mut dyn App) {
        *self.draw_request.write().unwrap() = None;
        self.has_frame_callback = false;
        // crates/eframe/src/native/wgpu_integration.rs

        let full_output = self
            .egui_state
            .process_events(|ctx| application.update(ctx));

        let surface_texture = self
            .wgpu_state
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");

        let surface_view = surface_texture
            .texture
            .create_view(&egui_wgpu::wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .wgpu_state
            .device
            .create_command_encoder(&egui_wgpu::wgpu::CommandEncoderDescriptor { label: None });

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [
                self.wgpu_state.surface_configuration.width,
                self.wgpu_state.surface_configuration.height,
            ],
            pixels_per_point: self.pixels_per_point(),
        };

        self.egui_state.draw(
            &self.wgpu_state.device,
            &self.wgpu_state.queue,
            &mut encoder,
            &surface_view,
            screen_descriptor,
            full_output.shapes,
            full_output.textures_delta,
        );
        self.wgpu_state.queue.submit(Some(encoder.finish()));

        self.layer
            .wl_surface()
            .frame(&self.queue_handle, self.layer.wl_surface().clone());
        surface_texture.present();

        // crates/egui-winit/src/lib.rs

        self.handle_platform(full_output.platform_output);

        if false {
            let pixels_per_point = self.pixels_per_point();
            for (id, view) in full_output.viewport_output {
                for cmd in view.commands {
                    match cmd {
                        ViewportCommand::IMEAllowed(v) => {
                            self.set_ime_allowed(v);
                        }
                        ViewportCommand::IMEPurpose(p) => self.set_ime_purpose(match p {
                            egui::viewport::IMEPurpose::Password => ImePurpose::Password,
                            egui::viewport::IMEPurpose::Terminal => ImePurpose::Terminal,
                            egui::viewport::IMEPurpose::Normal => ImePurpose::Normal,
                        }),
                        ViewportCommand::IMERect(rect) => {
                            self.set_ime_cursor_area(
                                PhysicalPosition::new(
                                    pixels_per_point * rect.min.x,
                                    pixels_per_point * rect.min.y,
                                )
                                .into(),
                                PhysicalSize::new(
                                    pixels_per_point * rect.size().x,
                                    pixels_per_point * rect.size().y,
                                )
                                .into(),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

delegate_registry!(WgpuLayerShellState);
impl ProvidesRegistryState for WgpuLayerShellState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_output!(WgpuLayerShellState);
impl OutputHandler for WgpuLayerShellState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

delegate_compositor!(WgpuLayerShellState);
impl CompositorHandler for WgpuLayerShellState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.has_frame_callback = true;
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

delegate_layer!(WgpuLayerShellState);
impl LayerShellHandler for WgpuLayerShellState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if !self.is_configured {
            self.is_configured = true;
            self.has_frame_callback = true;
            *self.draw_request.write().unwrap() = Some(Instant::now());
        }

        self.wgpu_state
            .resize(configure.new_size.0, configure.new_size.1);

        self.egui_state
            .set_size(configure.new_size.0, configure.new_size.1);
    }
}
delegate_seat!(WgpuLayerShellState);
impl SeatHandler for WgpuLayerShellState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seat_map.insert(seat.id(), Default::default());
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let seat_state = match self.seat_map.get_mut(&seat.id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_seat::new_capability for unknown seat");
                return;
            }
        };

        match capability {
            Capability::Pointer if self.pointer.is_none() => {
                let pointer = self
                    .seat_state
                    .get_pointer(qh, &seat)
                    .expect("Failed to create pointer");
                self.pointer = Some(pointer);
            }
            Capability::Keyboard if self.keyboard.is_none() => {
                self.keyboard = Some(
                    self.seat_state
                        .get_keyboard_with_repeat(
                            qh,
                            &seat,
                            None,
                            self.loop_handle.clone(),
                            Box::new(|state, _wl_kbd, event| {
                                handle_key_press(event, true, &mut state.egui_state.input());
                            }),
                        )
                        .expect("Failed to create keyboard"),
                );
            }
            _ => {}
        }

        if let Some(text_input_state) = seat_state
            .text_input
            .is_none()
            .then_some(self.window_text_input_state.as_ref())
            .flatten()
        {
            seat_state.text_input = Some(Arc::new(text_input_state.get_text_input(
                &seat,
                &qh,
                TextInputData::default(),
            )));
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer if self.pointer.is_some() => {
                self.pointer.take().unwrap().release();
            }
            Capability::Keyboard if self.keyboard.is_some() => {
                self.keyboard.take().unwrap().release();
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seat_map.remove(&seat.id());
    }
}

// delegate_dispatch!(WgpuLayerShellState: [ExtBackgroundEffectManagerV1: ()] => WgpuLayerShellState);
