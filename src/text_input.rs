use core::fmt;
use std::ops::Deref;

use dpi::{LogicalPosition, LogicalSize, Position, Size};
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use sctk::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ContentHint, ContentPurpose, Event as TextInputEvent, ZwpTextInputV3,
};
use tracing::warn;

use crate::layer_shell::WgpuLayerShellState;

#[derive(Debug)]
pub struct TextInputState {
    text_input_manager: ZwpTextInputManagerV3,
}

impl TextInputState {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WgpuLayerShellState>,
    ) -> Result<Self, BindError> {
        let text_input_manager = globals
            .bind::<ZwpTextInputManagerV3, WgpuLayerShellState, GlobalData>(
                queue_handle,
                1..=1,
                GlobalData,
            )?;
        Ok(Self { text_input_manager })
    }
}

impl Deref for TextInputState {
    type Target = ZwpTextInputManagerV3;

    fn deref(&self) -> &Self::Target {
        &self.text_input_manager
    }
}

impl Dispatch<ZwpTextInputManagerV3, GlobalData, WgpuLayerShellState> for TextInputState {
    fn event(
        _state: &mut WgpuLayerShellState,
        _proxy: &ZwpTextInputManagerV3,
        _event: <ZwpTextInputManagerV3 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WgpuLayerShellState>,
    ) {
    }
}

impl Dispatch<ZwpTextInputV3, TextInputData, WgpuLayerShellState> for TextInputState {
    fn event(
        state: &mut WgpuLayerShellState,
        text_input: &ZwpTextInputV3,
        event: <ZwpTextInputV3 as Proxy>::Event,
        data: &TextInputData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WgpuLayerShellState>,
    ) {
        let mut text_input_data = data.inner.lock().unwrap();
        match event {
            TextInputEvent::Enter { surface } => {
                text_input_data.surface = Some(surface);

                if let Some(text_input_state) = &mut state.text_input_state {
                    text_input.set_state(Some(text_input_state), true);
                    // The input method doesn't have to reply anything, so a synthetic event
                    // carrying an empty state notifies the application about its presence.
                    state.egui_state.ime_event_enable();
                }

                state.text_input_entered(text_input);
            }
            TextInputEvent::Leave { surface } => {
                text_input_data.surface = None;

                // Always issue a disable.
                text_input.disable();
                text_input.commit();

                state.text_input_left(text_input);
                state.egui_state.ime_event_disable();
            }
            TextInputEvent::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                let text = text.unwrap_or_default();
                let cursor_begin = usize::try_from(cursor_begin)
                    .ok()
                    .and_then(|idx| text.is_char_boundary(idx).then_some(idx));
                let cursor_end = usize::try_from(cursor_end)
                    .ok()
                    .and_then(|idx| text.is_char_boundary(idx).then_some(idx));

                text_input_data.pending_preedit = Some(Preedit {
                    text,
                    cursor_begin,
                    cursor_end,
                })
            }
            TextInputEvent::CommitString { text } => {
                text_input_data.pending_preedit = None;
                text_input_data.pending_commit = text;
            }
            TextInputEvent::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                text_input_data.pending_delete = Some(DeleteSurroundingText {
                    before: before_length as usize,
                    after: after_length as usize,
                });
            }
            TextInputEvent::Done { .. } => {
                // The events are sent to the user separately, so
                // CAUTION: events must always arrive in the order compatible with the application
                // order specified by the text-input-v3 protocol:
                //
                // As of version 1:
                // 1. Replace existing preedit string with the cursor.
                // 2. Delete requested surrounding text.
                // 3. Insert commit string with the cursor at its end.
                // 4. Calculate surrounding text to send.
                // 5. Insert new preedit text in cursor position.
                // 6. Place cursor inside preedit text.

                if let Some(DeleteSurroundingText { before, after }) =
                    text_input_data.pending_delete
                {
                    // state.events_sink.push_window_event(
                    //     WindowEvent::Ime(Ime::DeleteSurrounding {
                    //         before_bytes: before,
                    //         after_bytes: after,
                    //     }),
                    //     window_id,
                    // );
                }

                // Clear preedit, unless all we'll be doing next is sending a new preedit.
                if text_input_data.pending_commit.is_some()
                    || text_input_data.pending_preedit.is_none()
                {
                    state.egui_state.ime_event_disable();
                }

                // Send `Commit`.
                if let Some(text) = text_input_data.pending_commit.take() {
                    state
                        .egui_state
                        .push_event(egui::Event::Ime(egui::ImeEvent::Commit(text)));
                }

                // Send preedit.
                if let Some(preedit) = text_input_data.pending_preedit.take() {
                    let cursor_range = preedit
                        .cursor_begin
                        .map(|b| (b, preedit.cursor_end.unwrap_or(b)));
                    state.egui_state.ime_event_enable();
                    state
                        .egui_state
                        .push_event(egui::Event::Ime(egui::ImeEvent::Preedit(preedit.text)));
                }
            }
            _ => {}
        }
    }
}

pub trait ZwpTextInputV3Ext {
    /// Applies the entire state atomically to the input method. It will skip the "enable" request
    /// if `already_enabled` is `true`.
    fn set_state(&self, state: Option<&ClientState>, send_enable: bool);
}

impl ZwpTextInputV3Ext for ZwpTextInputV3 {
    fn set_state(&self, state: Option<&ClientState>, send_enable: bool) {
        let state = match state {
            Some(state) => state,
            None => {
                self.disable();
                self.commit();
                return;
            }
        };

        if send_enable {
            self.enable();
        }

        if let Some(content_type) = state.content_type() {
            self.set_content_type(content_type.hint, content_type.purpose);
        }

        if let Some((position, size)) = state.cursor_area() {
            let (x, y) = (position.x as i32, position.y as i32);
            let (width, height) = (size.width as i32, size.height as i32);
            // The same cursor can be applied on different seats.
            // It's the compositor's responsibility to make sure that any present popups don't
            // overlap.
            self.set_cursor_rectangle(x, y, width, height);
        }

        if let Some(surrounding) = state.surrounding_text() {
            self.set_surrounding_text(
                surrounding.text().into(),
                surrounding.cursor() as i32,
                surrounding.anchor() as i32,
            );
        }

        self.commit();
    }
}

/// The Data associated with the text input.
#[derive(Default)]
pub struct TextInputData {
    inner: std::sync::Mutex<TextInputDataInner>,
}

#[derive(Default)]
pub struct TextInputDataInner {
    /// The `WlSurface` we're performing input to.
    surface: Option<WlSurface>,

    /// The commit to submit on `done`.
    pending_commit: Option<String>,

    /// The preedit to submit on `done`.
    pending_preedit: Option<Preedit>,

    /// The text around the cursor to delete on `done`
    pending_delete: Option<DeleteSurroundingText>,
}

/// The state of the preedit.
#[derive(Clone)]
struct Preedit {
    text: String,
    cursor_begin: Option<usize>,
    cursor_end: Option<usize>,
}

/// The delete request
#[derive(Clone)]
struct DeleteSurroundingText {
    /// Bytes before cursor
    before: usize,
    /// Bytes after cursor
    after: usize,
}

/// State change requested by the application.
///
/// This is a version that uses text_input abstractions translated from the ones used in
/// winit::core::window::ImeStateChange.
///
/// Fields that are initially set to None are unsupported capabilities
/// and trying to set them raises an error.
#[derive(Debug, PartialEq, Clone)]
pub struct ClientState {
    capabilities: ImeCapabilities,
    content_type: ContentType,
    /// The IME cursor area which should not be covered by the input method popup.
    cursor_area: (LogicalPosition<u32>, LogicalSize<u32>),

    /// The `ImeSurroundingText` struct is based on the Wayland model.
    /// When this changes, another struct might be needed.
    surrounding_text: ImeSurroundingText,
}
/// Request to send to IME.
#[derive(Debug, PartialEq, Clone)]
pub enum ImeRequest {
    /// Enable the IME with the [`ImeCapabilities`] and [`ImeRequestData`] as initial state. When
    /// the [`ImeRequestData`] is **not** matching capabilities fully, the default values will be
    /// used instead.
    ///
    /// **Requesting to update data matching not enabled capabilities will result in update
    /// being ignored.** The winit backend in such cases is recommended to log a warning. This
    /// appiles to both [`ImeRequest::Enable`] and [`ImeRequest::Update`]. For details on
    /// capabilities refer to [`ImeCapabilities`].
    ///
    /// To update the [`ImeCapabilities`], the IME must be disabled and then re-enabled.
    Enable(ImeEnableRequest),
    /// Update the state of already enabled IME. Issuing this request before [`ImeRequest::Enable`]
    /// will result in error.
    Update(ImeRequestData),
    /// Disable the IME.
    ///
    /// **The disable request can not fail**.
    Disable,
}
/// Initial IME request.
#[derive(Debug, Clone, PartialEq)]
pub struct ImeEnableRequest {
    capabilities: ImeCapabilities,
    request_data: ImeRequestData,
}

/// Error from sending request to IME with
/// [`Window::request_ime_update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImeRequestError {
    /// IME is not yet enabled.
    NotEnabled,
    /// IME is already enabled.
    AlreadyEnabled,
    /// Not supported.
    NotSupported,
}

impl fmt::Display for ImeRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImeRequestError::NotEnabled => write!(f, "ime is not enabled."),
            ImeRequestError::AlreadyEnabled => write!(f, "ime is already enabled."),
            ImeRequestError::NotSupported => write!(f, "ime is not supported."),
        }
    }
}

impl ImeEnableRequest {
    /// Create request for the [`ImeRequest::Enable`]
    ///
    /// This will return [`None`] if some capability was requested but its initial value was not
    /// set by the user or value was set by the user, but capability not requested.
    pub fn new(capabilities: ImeCapabilities, request_data: ImeRequestData) -> Option<Self> {
        if capabilities.cursor_area() ^ request_data.cursor_area.is_some() {
            return None;
        }

        if capabilities.hint_and_purpose() ^ request_data.hint_and_purpose.is_some() {
            return None;
        }

        if capabilities.surrounding_text() ^ request_data.surrounding_text.is_some() {
            return None;
        }
        Some(Self {
            capabilities,
            request_data,
        })
    }

    /// [`ImeCapabilities`] to enable.
    pub const fn capabilities(&self) -> &ImeCapabilities {
        &self.capabilities
    }

    /// Request data attached to request.
    pub const fn request_data(&self) -> &ImeRequestData {
        &self.request_data
    }

    /// Destruct [`ImeEnableRequest`]  into its raw parts.
    pub fn into_raw(self) -> (ImeCapabilities, ImeRequestData) {
        (self.capabilities, self.request_data)
    }
}

/// IME capabilities supported by client.
///
/// For example, if the client doesn't support [`ImeCapabilities::cursor_area()`], then not enabling
/// it will make IME hide the popup window instead of placing it arbitrary over the
/// client's window surface.
///
/// When the capability is not enabled or not supported by the IME, trying to update its'
/// corresponding data with [`ImeRequest`] will be ignored.
///
/// New capabilities may be added to this struct in the future.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ImeCapabilities(ImeCapabilitiesFlags);

impl ImeCapabilities {
    /// Returns a new empty set of capabilities.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks `hint and purpose` as supported.
    ///
    /// For more details see [`ImeRequestData::with_hint_and_purpose`].
    pub const fn with_hint_and_purpose(self) -> Self {
        Self(self.0.union(ImeCapabilitiesFlags::HINT_AND_PURPOSE))
    }

    /// Marks `hint and purpose` as unsupported.
    ///
    /// For more details see [`ImeRequestData::with_hint_and_purpose`].
    pub const fn without_hint_and_purpose(self) -> Self {
        Self(self.0.difference(ImeCapabilitiesFlags::HINT_AND_PURPOSE))
    }

    /// Returns `true` if `hint and purpose` is supported.
    pub const fn hint_and_purpose(&self) -> bool {
        self.0.contains(ImeCapabilitiesFlags::HINT_AND_PURPOSE)
    }

    /// Marks `cursor_area` as supported.
    ///
    /// For more details see [`ImeRequestData::with_cursor_area`].
    pub const fn with_cursor_area(self) -> Self {
        Self(self.0.union(ImeCapabilitiesFlags::CURSOR_AREA))
    }

    /// Marks `cursor_area` as unsupported.
    ///
    /// For more details see [`ImeRequestData::with_cursor_area`].
    pub const fn without_cursor_area(self) -> Self {
        Self(self.0.difference(ImeCapabilitiesFlags::CURSOR_AREA))
    }

    /// Returns `true` if `cursor_area` is supported.
    pub const fn cursor_area(&self) -> bool {
        self.0.contains(ImeCapabilitiesFlags::CURSOR_AREA)
    }

    /// Marks `surrounding_text` as supported.
    ///
    /// For more details see [`ImeRequestData::with_surrounding_text`].
    pub const fn with_surrounding_text(self) -> Self {
        Self(self.0.union(ImeCapabilitiesFlags::SURROUNDING_TEXT))
    }

    /// Marks `surrounding_text` as unsupported.
    ///
    /// For more details see [`ImeRequestData::with_surrounding_text`].
    pub const fn without_surrounding_text(self) -> Self {
        Self(self.0.difference(ImeCapabilitiesFlags::SURROUNDING_TEXT))
    }

    /// Returns `true` if `surrounding_text` is supported.
    pub const fn surrounding_text(&self) -> bool {
        self.0.contains(ImeCapabilitiesFlags::SURROUNDING_TEXT)
    }
}

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub(crate) struct ImeCapabilitiesFlags : u8 {
        /// Client supports setting IME hint and purpose.
        const HINT_AND_PURPOSE = 1 << 0;
        /// Client supports reporting cursor area for IME popup to
        /// appear.
        const CURSOR_AREA = 1 << 1;
        /// Client supports reporting the text around the caret
        const SURROUNDING_TEXT = 1 << 2;
    }
}

impl WgpuLayerShellState {
    pub fn scale_factor(&self) -> f64 {
        1.0
    }
    /// Register text input on the top-level.
    #[inline]
    pub fn text_input_entered(&mut self, text_input: &ZwpTextInputV3) {
        if !self.text_inputs.iter().any(|t| t == text_input) {
            self.text_inputs.push(text_input.clone());
        }
    }

    /// The text input left the top-level.
    #[inline]
    pub fn text_input_left(&mut self, text_input: &ZwpTextInputV3) {
        if let Some(position) = self.text_inputs.iter().position(|t| t == text_input) {
            self.text_inputs.remove(position);
        }
    }

    /// Atomically update input method state.
    ///
    /// Returns `None` if an input method state haven't changed. Alternatively `Some(true)` and
    /// `Some(false)` is returned respectfully.
    pub fn request_ime_update(
        &mut self,
        request: ImeRequest,
    ) -> Result<Option<bool>, ImeRequestError> {
        let state_change = match request {
            ImeRequest::Enable(enable) => {
                let (capabilities, request_data) = enable.into_raw();

                if self.window_text_input_state.is_some() {
                    return Err(ImeRequestError::AlreadyEnabled);
                }

                self.text_input_state = Some(TextInputClientState::new(
                    capabilities,
                    request_data,
                    self.scale_factor(),
                ));
                true
            }
            ImeRequest::Update(request_data) => {
                let scale_factor = self.scale_factor();
                if let Some(text_input_state) = self.text_input_state.as_mut() {
                    text_input_state.update(request_data, scale_factor);
                } else {
                    return Err(ImeRequestError::NotEnabled);
                }
                false
            }
            ImeRequest::Disable => {
                self.window_text_input_state = None;
                true
            }
        };

        // Only one input method may be active per (seat, surface),
        // but there may be multiple seats focused on a surface,
        // resulting in multiple text input objects.
        //
        // WARNING: this doesn't actually handle different seats with independent cursors. There's
        // no API to set a per-seat input method state, so they all share a single state.
        for text_input in &self.text_inputs {
            text_input.set_state(self.text_input_state.as_ref(), state_change);
        }

        if state_change {
            Ok(Some(self.window_text_input_state.is_some()))
        } else {
            Ok(None)
        }
    }
}

pub type TextInputClientState = ClientState;

impl ClientState {
    pub fn new(
        capabilities: ImeCapabilities,
        request_data: ImeRequestData,
        scale_factor: f64,
    ) -> Self {
        let mut this = Self {
            capabilities,
            content_type: Default::default(),
            cursor_area: Default::default(),
            surrounding_text: ImeSurroundingText::new(String::new(), 0, 0).unwrap(),
        };

        let unsupported_flags = capabilities
            .without_hint_and_purpose()
            .without_cursor_area()
            .without_surrounding_text();

        if unsupported_flags != ImeCapabilities::new() {
            warn!(
                "Backend doesn't support all requested IME capabilities: {:?}.\n Ignoring.",
                unsupported_flags
            );
        }

        this.update(request_data, scale_factor);
        this
    }

    pub fn capabilities(&self) -> ImeCapabilities {
        self.capabilities
    }

    /// Updates the fields of the state which are present in update_fields.
    pub fn update(&mut self, request_data: ImeRequestData, scale_factor: f64) {
        if let Some((hint, purpose)) = request_data
            .hint_and_purpose
            .filter(|_| self.capabilities.hint_and_purpose())
        {
            self.content_type = (hint, purpose).into();
        }

        if let Some((position, size)) = request_data.cursor_area {
            if self.capabilities.cursor_area() {
                let position: LogicalPosition<u32> = position.to_logical(scale_factor);
                let size: LogicalSize<u32> = size.to_logical(scale_factor);
                self.cursor_area = (position, size);
            } else {
                warn!("discarding IME cursor area update without capability enabled.");
            }
        }

        if let Some(surrounding) = request_data.surrounding_text {
            if self.capabilities.surrounding_text() {
                self.surrounding_text = surrounding;
            } else {
                warn!("discarding IME surrounding text update without capability enabled.");
            }
        }
    }

    pub fn content_type(&self) -> Option<ContentType> {
        self.capabilities
            .hint_and_purpose()
            .then_some(self.content_type)
    }

    pub fn cursor_area(&self) -> Option<(LogicalPosition<u32>, LogicalSize<u32>)> {
        self.capabilities.cursor_area().then_some(self.cursor_area)
    }

    pub fn surrounding_text(&self) -> Option<&ImeSurroundingText> {
        self.capabilities
            .surrounding_text()
            .then_some(&self.surrounding_text)
    }
}

/// Arguments to content_type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentType {
    /// Text input hint.
    hint: ContentHint,
    /// Text input purpose.
    purpose: ContentPurpose,
}

/// The two options influence each other, so they must be converted together.
impl From<(ImeHint, ImePurpose)> for ContentType {
    fn from((hint, purpose): (ImeHint, ImePurpose)) -> Self {
        let purpose = match purpose {
            ImePurpose::Password => ContentPurpose::Password,
            ImePurpose::Terminal => ContentPurpose::Terminal,
            ImePurpose::Phone => ContentPurpose::Phone,
            ImePurpose::Number => ContentPurpose::Number,
            ImePurpose::Url => ContentPurpose::Url,
            ImePurpose::Email => ContentPurpose::Email,
            ImePurpose::Pin => ContentPurpose::Pin,
            ImePurpose::Date => ContentPurpose::Date,
            ImePurpose::Time => ContentPurpose::Time,
            ImePurpose::DateTime => ContentPurpose::Datetime,
            _ => ContentPurpose::Normal,
        };

        let base_hint = match purpose {
            // Before the hint API was introduced, password  purpose guaranteed the
            // sensitive hint. Keep this behaviour for the sake of backwards compatibility.
            ContentPurpose::Password | ContentPurpose::Pin => ContentHint::SensitiveData,
            _ => ContentHint::None,
        };

        let mut new_hint = base_hint;
        if hint.contains(ImeHint::COMPLETION) {
            new_hint |= ContentHint::Completion;
        }
        if hint.contains(ImeHint::SPELLCHECK) {
            new_hint |= ContentHint::Spellcheck;
        }
        if hint.contains(ImeHint::AUTO_CAPITALIZATION) {
            new_hint |= ContentHint::AutoCapitalization;
        }
        if hint.contains(ImeHint::LOWERCASE) {
            new_hint |= ContentHint::Lowercase;
        }
        if hint.contains(ImeHint::UPPERCASE) {
            new_hint |= ContentHint::Uppercase;
        }
        if hint.contains(ImeHint::TITLECASE) {
            new_hint |= ContentHint::Titlecase;
        }
        if hint.contains(ImeHint::HIDDEN_TEXT) {
            new_hint |= ContentHint::HiddenText;
        }
        if hint.contains(ImeHint::SENSITIVE_DATA) {
            new_hint |= ContentHint::SensitiveData;
        }
        if hint.contains(ImeHint::LATIN) {
            new_hint |= ContentHint::Latin;
        }
        if hint.contains(ImeHint::MULTILINE) {
            new_hint |= ContentHint::Multiline;
        }

        Self {
            hint: new_hint,
            purpose,
        }
    }
}

impl Default for ContentType {
    fn default() -> Self {
        ContentType {
            purpose: ContentPurpose::Normal,
            hint: ContentHint::None,
        }
    }
}

bitflags! {
    /// IME hints
    ///
    /// The hint should reflect the desired behaviour of the IME
    /// while entering text.
    /// The purpose may improve UX by optimizing the IME for the specific use case,
    /// beyond just the general data type specified in `ImePurpose`.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Windows / X11 / macOS / Orbital:** Unsupported.
    #[non_exhaustive]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct ImeHint: u32 {
        /// No special behaviour.
        const NONE = 0;
        /// Suggest word completions.
        const COMPLETION = 0x1;
        /// Suggest word corrections.
        const SPELLCHECK = 0x2;
        /// Switch to uppercase letters at the start of a sentence.
        const AUTO_CAPITALIZATION = 0x4;
        /// Prefer lowercase letters.
        const LOWERCASE = 0x8;
        /// Prefer uppercase letters.
        const UPPERCASE = 0x10;
        /// Prefer casing for titles and headings (can be language dependent).
        const TITLECASE = 0x20;
        /// Characters should be hidden.
        ///
        /// This may prevent e.g. layout switching with some IMEs, unless hint is disabled.
        const HIDDEN_TEXT = 0x40;
        /// Typed text should not be stored.
        const SENSITIVE_DATA = 0x80;
        /// Just Latin characters should be entered.
        const LATIN = 0x100;
        /// The text input is multiline.
        const MULTILINE = 0x200;
    }
}

/// Generic IME purposes for use in [`Window::set_ime_purpose`].
///
/// The purpose should reflect the kind of data to be entered.
/// The purpose may improve UX by optimizing the IME for the specific use case,
/// for example showing relevant characters and hiding unneeded ones,
/// or changing the icon of the confrirmation button,
/// if winit can express the purpose to the platform and the platform reacts accordingly.
///
/// ## Platform-specific
///
/// - **iOS / Android / Web / Windows / X11 / macOS / Orbital:** Unsupported.
#[non_exhaustive]
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ImePurpose {
    /// No special purpose for the IME (default).
    Normal,
    /// The IME is used for password input.
    /// The IME will treat the contents as sensitive.
    Password,
    /// The IME is used to input into a terminal.
    ///
    /// For example, that could alter OSK on Wayland to show extra buttons.
    Terminal,
    /// Number (including decimal separator and sign)
    Number,
    /// Phone number
    Phone,
    /// URL
    Url,
    /// Email address
    Email,
    /// Password composed only of digits (treated as sensitive data)
    Pin,
    /// Date
    Date,
    /// Time
    Time,
    /// Date and time
    DateTime,
}

impl Default for ImePurpose {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum ImeSurroundingTextError {
    /// Text exceeds 4000 bytes
    TextTooLong,
    /// Cursor not on a code point boundary, or past the end of text.
    CursorBadPosition,
    /// Anchor not on a code point boundary, or past the end of text.
    AnchorBadPosition,
}

/// Defines the text surrounding the caret
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ImeSurroundingText {
    /// An excerpt of the text present in the text input field, excluding preedit.
    text: String,
    /// The position of the caret, in bytes from the beginning of the string
    cursor: usize,
    /// The position of the other end of selection, in bytes.
    /// With no selection, it should be the same as the cursor.
    anchor: usize,
}

impl ImeSurroundingText {
    /// The maximum size of the text excerpt.
    pub const MAX_TEXT_BYTES: usize = 4000;
    /// Defines the text surrounding the cursor and the selection within it.
    ///
    /// `text`: An excerpt of the text present in the text input field, excluding preedit.
    /// It must be limited to 4000 bytes due to backend constraints.
    /// `cursor`: The position of the caret, in bytes from the beginning of the string.
    /// `anchor: The position of the other end of selection, in bytes.
    /// With no selection, it should be the same as the cursor.
    ///
    /// This may fail if the byte indices don't fall on code point boundaries,
    /// or if the text is too long.
    ///
    /// ## Examples:
    ///
    /// A text field containing `foo|bar` where `|` denotes the caret would correspond to a value
    /// obtained by:
    ///
    /// ```
    /// # use winit_core::window::ImeSurroundingText;
    /// let s = ImeSurroundingText::new("foobar".into(), 3, 3).unwrap();
    /// ```
    ///
    /// Because preedit is excluded from the text string, a text field containing `foo[baz|]bar`
    /// where `|` denotes the caret and [baz|] is the preedit would be created in exactly the same
    /// way.
    pub fn new(
        text: String,
        cursor: usize,
        anchor: usize,
    ) -> Result<Self, ImeSurroundingTextError> {
        let text = if text.len() < 4000 {
            text
        } else {
            return Err(ImeSurroundingTextError::TextTooLong);
        };

        let cursor = if text.is_char_boundary(cursor) && cursor <= text.len() {
            cursor
        } else {
            return Err(ImeSurroundingTextError::CursorBadPosition);
        };

        let anchor = if text.is_char_boundary(anchor) && anchor <= text.len() {
            anchor
        } else {
            return Err(ImeSurroundingTextError::AnchorBadPosition);
        };

        Ok(Self {
            text,
            cursor,
            anchor,
        })
    }

    /// Consumes the object, releasing the text string only.
    /// Use this call in the backend to avoid an extra clone when submitting the surrounding text.
    pub fn into_text(self) -> String {
        self.text
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn anchor(&self) -> usize {
        self.anchor
    }
}

/// The [`ImeRequest`] data to communicate to system's IME.
///
/// This applies multiple IME state properties at once.
/// Fields set to `None` are not updated and the previously sent
/// value is reused.
#[non_exhaustive]
#[derive(Debug, PartialEq, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ImeRequestData {
    /// Text input hint and purpose.
    ///
    /// To support updating it, enable [`ImeCapabilities::HINT_AND_PURPOSE`].
    pub hint_and_purpose: Option<(ImeHint, ImePurpose)>,
    /// The IME cursor area which should not be covered by the input method popup.
    ///
    /// To support updating it, enable [`ImeCapabilities::CURSOR_AREA`].
    pub cursor_area: Option<(Position, Size)>,
    /// The text surrounding the caret
    ///
    /// To support updating it, enable [`ImeCapabilities::SURROUNDING_TEXT`].
    pub surrounding_text: Option<ImeSurroundingText>,
}

impl ImeRequestData {
    /// Sets the hint and purpose of the current text input content.
    pub fn with_hint_and_purpose(self, hint: ImeHint, purpose: ImePurpose) -> Self {
        Self {
            hint_and_purpose: Some((hint, purpose)),
            ..self
        }
    }

    /// Sets the IME cursor editing area.
    ///
    /// The `position` is the top left corner of that area
    /// in surface coordinates and `size` is the size of this area starting from the position. An
    /// example of such area could be a input field in the UI or line in the editor.
    ///
    /// The windowing system could place a candidate box close to that area, but try to not obscure
    /// the specified area, so the user input to it stays visible.
    ///
    /// The candidate box is the window / popup / overlay that allows you to select the desired
    /// characters. The look of this box may differ between input devices, even on the same
    /// platform.
    ///
    /// (Apple's official term is "candidate window", see their [chinese] and [japanese] guides).
    ///
    /// ## Example
    ///
    /// ```no_run
    /// # use dpi::{LogicalPosition, PhysicalPosition, LogicalSize, PhysicalSize};
    /// # use winit_core::window::ImeRequestData;
    /// # fn scope(ime_request_data: ImeRequestData) {
    /// // Specify the position in logical dimensions like this:
    /// let ime_request_data = ime_request_data.with_cursor_area(
    ///     LogicalPosition::new(400.0, 200.0).into(),
    ///     LogicalSize::new(100, 100).into(),
    /// );
    ///
    /// // Or specify the position in physical dimensions like this:
    /// let ime_request_data = ime_request_data.with_cursor_area(
    ///     PhysicalPosition::new(400, 200).into(),
    ///     PhysicalSize::new(100, 100).into(),
    /// );
    /// # }
    /// ```
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Web / Orbital:** Unsupported.
    ///
    /// [chinese]: https://support.apple.com/guide/chinese-input-method/use-the-candidate-window-cim12992/104/mac/12.0
    /// [japanese]: https://support.apple.com/guide/japanese-input-method/use-the-candidate-window-jpim10262/6.3/mac/12.0
    pub fn with_cursor_area(self, position: Position, size: Size) -> Self {
        Self {
            cursor_area: Some((position, size)),
            ..self
        }
    }

    /// Describes the text surrounding the caret.
    ///
    /// The IME can then continue providing suggestions for the continuation of the existing text,
    /// as well as can erase text more accurately, for example glyphs composed of multiple code
    /// points.
    pub fn with_surrounding_text(self, surrounding_text: ImeSurroundingText) -> Self {
        Self {
            surrounding_text: Some(surrounding_text),
            ..self
        }
    }
}

delegate_dispatch!(WgpuLayerShellState: [ZwpTextInputManagerV3: GlobalData] => TextInputState);
delegate_dispatch!(WgpuLayerShellState: [ZwpTextInputV3: TextInputData] => TextInputState);
