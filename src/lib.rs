//! [`egui`] bindings for [`winit`](https://github.com/rust-windowing/winit).
//!
//! The library translates winit events to egui, handled copy/paste,
//! updates the cursor, open links clicked in egui, etc.
//!
//! ## Feature flags
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]
//!

#![allow(clippy::manual_range_contains)]


// #[cfg(feature = "accesskit")]
// pub use accesskit_winit;
pub use egui;
// #[cfg(feature = "accesskit")]
// use egui::accesskit;
pub use tao;
use tao::clipboard;
use tao::event::{MouseButton, TouchPhase};

//pub mod clipboard;
mod window_settings;

pub use window_settings::WindowSettings;

use tao::keyboard::KeyCode;

pub fn native_pixels_per_point(window: &tao::window::Window) -> f32 {
    window.scale_factor() as f32
}

pub fn screen_size_in_pixels(window: &tao::window::Window) -> egui::Vec2 {
    let size = window.inner_size();
    egui::vec2(size.width as f32, size.height as f32)
}

// ----------------------------------------------------------------------------

#[must_use]
pub struct EventResponse {
    /// If true, egui consumed this event, i.e. wants exclusive use of this event
    /// (e.g. a mouse click on an egui window, or entering text into a text field).
    ///
    /// For instance, if you use egui for a game, you should only
    /// pass on the events to your game when [`Self::consumed`] is `false.
    ///
    /// Note that egui uses `tab` to move focus between elements, so this will always be `true` for tabs.
    pub consumed: bool,

    /// Do we need an egui refresh because of this event?
    pub repaint: bool,
}

// ----------------------------------------------------------------------------

/// Handles the integration between egui and winit.
pub struct State {
    start_time: instant::Instant,
    egui_input: egui::RawInput,
    pointer_pos_in_points: Option<egui::Pos2>,
    any_pointer_button_down: bool,
    current_cursor_icon: Option<egui::CursorIcon>,

    /// What egui uses.
    current_pixels_per_point: f32,

    clipboard: clipboard::Clipboard,

    /// If `true`, mouse inputs will be treated as touches.
    /// Useful for debugging touch support in egui.
    ///
    /// Creates duplicate touches, if real touch inputs are coming.
    simulate_touch_screen: bool,

    /// Is Some(…) when a touch is being translated to a pointer.
    ///
    /// Only one touch will be interpreted as pointer at any time.
    pointer_touch_id: Option<u64>,

    // track ime state
    //input_method_editor_started: bool,

    //#[cfg(feature = "accesskit")]
    //accesskit: Option<accesskit_tao::Adapter>,
}

impl State {
    pub fn new() -> Self {
        let egui_input = egui::RawInput {
            focused: false, // winit will tell us when we have focus
            ..Default::default()
        };

        Self {
            start_time: instant::Instant::now(),
            egui_input,
            pointer_pos_in_points: None,
            any_pointer_button_down: false,
            current_cursor_icon: None,
            current_pixels_per_point: 1.0,

            clipboard: clipboard::Clipboard::new(),

            simulate_touch_screen: false,
            pointer_touch_id: None,

            //input_method_editor_started: false,

            //#[cfg(feature = "accesskit")]
            //accesskit: None,
        }
    }

    //#[cfg(feature = "accesskit")]
    //pub fn init_accesskit<T: From<accesskit_tao::ActionRequestEvent> + Send>(
    //    &mut self,
    //    window: &tao::window::Window,
    //    event_loop_proxy: tao::event_loop::EventLoopProxy<T>,
    //    initial_tree_update_factory: impl 'static + FnOnce() -> accesskit::TreeUpdate + Send,
    //) {
    //    self.accesskit = Some(accesskit_tao::Adapter::new(
    //        window,
    //        initial_tree_update_factory,
    //        event_loop_proxy,
    //    ));
    //}

    /// Call this once a graphics context has been created to update the maximum texture dimensions
    /// that egui will use.
    pub fn set_max_texture_side(&mut self, max_texture_side: usize) {
        self.egui_input.max_texture_side = Some(max_texture_side);
    }

    /// Call this when a new native Window is created for rendering to initialize the `pixels_per_point`
    /// for that window.
    ///
    /// In particular, on Android it is necessary to call this after each `Resumed` lifecycle
    /// event, each time a new native window is created.
    ///
    /// Once this has been initialized for a new window then this state will be maintained by handling
    /// [`tao::event::WindowEvent::ScaleFactorChanged`] events.
    pub fn set_pixels_per_point(&mut self, pixels_per_point: f32) {
        self.egui_input.pixels_per_point = Some(pixels_per_point);
        self.current_pixels_per_point = pixels_per_point;
    }

    /// The number of physical pixels per logical point,
    /// as configured on the current egui context (see [`egui::Context::pixels_per_point`]).
    #[inline]
    pub fn pixels_per_point(&self) -> f32 {
        self.current_pixels_per_point
    }

    /// The current input state.
    /// This is changed by [`Self::on_event`] and cleared by [`Self::take_egui_input`].
    #[inline]
    pub fn egui_input(&self) -> &egui::RawInput {
        &self.egui_input
    }

    /// Prepare for a new frame by extracting the accumulated input,
    /// as well as setting [the time](egui::RawInput::time) and [screen rectangle](egui::RawInput::screen_rect).
    pub fn take_egui_input(&mut self, window: &tao::window::Window) -> egui::RawInput {
        let pixels_per_point = self.pixels_per_point();

        self.egui_input.time = Some(self.start_time.elapsed().as_secs_f64());

        // On Windows, a minimized window will have 0 width and height.
        // See: https://github.com/rust-windowing/winit/issues/208
        // This solves an issue where egui window positions would be changed when minimizing on Windows.
        let screen_size_in_pixels = screen_size_in_pixels(window);
        let screen_size_in_points = screen_size_in_pixels / pixels_per_point;
        self.egui_input.screen_rect =
            if screen_size_in_points.x > 0.0 && screen_size_in_points.y > 0.0 {
                Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    screen_size_in_points,
                ))
            } else {
                None
            };

        self.egui_input.take()
    }

    /// Call this when there is a new event.
    ///
    /// The result can be found in [`Self::egui_input`] and be extracted with [`Self::take_egui_input`].
    pub fn on_event(
        &mut self,
        egui_ctx: &egui::Context,
        event: &tao::event::WindowEvent<'_>,
    ) -> EventResponse {
        use tao::event::WindowEvent;
        match event {
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let pixels_per_point = *scale_factor as f32;
                self.egui_input.pixels_per_point = Some(pixels_per_point);
                self.current_pixels_per_point = pixels_per_point;
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.on_mouse_button_input(*state, *button);
                EventResponse {
                    repaint: true,
                    consumed: egui_ctx.wants_pointer_input(),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.on_mouse_wheel(*delta);
                EventResponse {
                    repaint: true,
                    consumed: egui_ctx.wants_pointer_input(),
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(*position);
                EventResponse {
                    repaint: true,
                    consumed: egui_ctx.is_using_pointer(),
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer_pos_in_points = None;
                self.egui_input.events.push(egui::Event::PointerGone);
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            // WindowEvent::TouchpadPressure {device_id, pressure, stage, ..  } => {} // TODO
            WindowEvent::Touch(touch) => {
                self.on_touch(touch);
                let consumed = match touch.phase {
                    tao::event::TouchPhase::Started
                    | tao::event::TouchPhase::Ended
                    | tao::event::TouchPhase::Cancelled => egui_ctx.wants_pointer_input(),
                    tao::event::TouchPhase::Moved => egui_ctx.is_using_pointer(),
                    _ => unreachable!()
                };
                EventResponse {
                    repaint: true,
                    consumed,
                }
            }
            /*
            WindowEvent::ReceivedCharacter(ch) => {
                // On Mac we get here when the user presses Cmd-C (copy), ctrl-W, etc.
                // We need to ignore these characters that are side-effects of commands.
                let is_mac_cmd = cfg!(target_os = "macos")
                    && (self.egui_input.modifiers.ctrl || self.egui_input.modifiers.mac_cmd);

                let consumed = if is_printable_char(*ch) && !is_mac_cmd {
                    self.egui_input
                        .events
                        .push(egui::Event::Text(ch.to_string()));
                    egui_ctx.wants_keyboard_input()
                } else {
                    false
                };
                EventResponse {
                    repaint: true,
                    consumed,
                }
            }
            WindowEvent::Ime(ime) => {
                // on Mac even Cmd-C is pressed during ime, a `c` is pushed to Preedit.
                // So no need to check is_mac_cmd.
                //
                // How winit produce `Ime::Enabled` and `Ime::Disabled` differs in MacOS
                // and Windows.
                //
                // - On Windows, before and after each Commit will produce an Enable/Disabled
                // event.
                // - On MacOS, only when user explicit enable/disable ime. No Disabled
                // after Commit.
                //
                // We use input_method_editor_started to manually insert CompositionStart
                // between Commits.
                match ime {
                    tao::event::Ime::Enabled | tao::event::Ime::Disabled => (),
                    tao::event::Ime::Commit(text) => {
                        self.input_method_editor_started = false;
                        self.egui_input
                            .events
                            .push(egui::Event::CompositionEnd(text.clone()));
                    }
                    tao::event::Ime::Preedit(text, ..) => {
                        if !self.input_method_editor_started {
                            self.input_method_editor_started = true;
                            self.egui_input.events.push(egui::Event::CompositionStart);
                        }
                        self.egui_input
                            .events
                            .push(egui::Event::CompositionUpdate(text.clone()));
                    }
                };

                EventResponse {
                    repaint: true,
                    consumed: egui_ctx.wants_keyboard_input(),
                }
            }
             */
            WindowEvent::ReceivedImeText(ch) => {
                // On Mac we get here when the user presses Cmd-C (copy), ctrl-W, etc.
                // We need to ignore these characters that are side-effects of commands.
                let is_mac_cmd = cfg!(target_os = "macos")
                    && (self.egui_input.modifiers.ctrl || self.egui_input.modifiers.mac_cmd);

                let consumed = if ch.chars().all(is_printable_char) && !is_mac_cmd {
                    self.egui_input
                        .events
                        .push(egui::Event::Text(ch.to_string()));
                    egui_ctx.wants_keyboard_input()
                } else {
                    false
                };
                EventResponse {
                    repaint: true,
                    consumed,
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.on_keyboard_input(event);
                let consumed = egui_ctx.wants_keyboard_input()
                    || event.physical_key == KeyCode::Tab;
                EventResponse {
                    repaint: true,
                    consumed,
                }
            }
            WindowEvent::Focused(focused) => {
                self.egui_input.focused = *focused;
                // We will not be given a KeyboardInput event when the modifiers are released while
                // the window does not have focus. Unset all modifier state to be safe.
                self.egui_input.modifiers = egui::Modifiers::default();
                self.egui_input
                    .events
                    .push(egui::Event::WindowFocused(*focused));
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::HoveredFile(path) => {
                self.egui_input.hovered_files.push(egui::HoveredFile {
                    path: Some(path.clone()),
                    ..Default::default()
                });
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::HoveredFileCancelled => {
                self.egui_input.hovered_files.clear();
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.egui_input.hovered_files.clear();
                self.egui_input.dropped_files.push(egui::DroppedFile {
                    path: Some(path.clone()),
                    ..Default::default()
                });
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::ModifiersChanged(state) => {
                self.egui_input.modifiers.alt = state.alt_key();
                self.egui_input.modifiers.ctrl = state.control_key();
                self.egui_input.modifiers.shift = state.shift_key();
                self.egui_input.modifiers.mac_cmd = cfg!(target_os = "macos") && state.super_key();
                self.egui_input.modifiers.command = if cfg!(target_os = "macos") {
                    state.super_key()
                } else {
                    state.control_key()
                };
                EventResponse {
                    repaint: true,
                    consumed: false,
                }
            }

            // Things that may require repaint:
            WindowEvent::CloseRequested
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::Destroyed
            //| WindowEvent::Occluded(_)
            | WindowEvent::Resized(_)
            | WindowEvent::ThemeChanged(_)
            | WindowEvent::TouchpadPressure { .. } => EventResponse {
                repaint: true,
                consumed: false,
            },

            // Things we completely ignore:
            _ => EventResponse {
                repaint: false,
                consumed: false,
            },

            //WindowEvent::TouchpadMagnify { delta, .. } => {
            //    // Positive delta values indicate magnification (zooming in).
            //    // Negative delta values indicate shrinking (zooming out).
            //    let zoom_factor = (*delta as f32).exp();
            //    self.egui_input.events.push(egui::Event::Zoom(zoom_factor));
            //    EventResponse {
            //        repaint: true,
            //        consumed: egui_ctx.wants_pointer_input(),
            //    }
            //}
        }
    }

    /// Call this when there is a new [`accesskit::ActionRequest`].
    ///
    /// The result can be found in [`Self::egui_input`] and be extracted with [`Self::take_egui_input`].
    //#[cfg(feature = "accesskit")]
    //pub fn on_accesskit_action_request(&mut self, request: accesskit::ActionRequest) {
    //    self.egui_input
    //        .events
    //        .push(egui::Event::AccessKitActionRequest(request));
    //}

    fn on_mouse_button_input(
        &mut self,
        state: tao::event::ElementState,
        button: tao::event::MouseButton,
    ) {
        if let Some(pos) = self.pointer_pos_in_points {
            if let Some(button) = translate_mouse_button(button) {
                let pressed = state == tao::event::ElementState::Pressed;

                self.egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: self.egui_input.modifiers,
                });

                if self.simulate_touch_screen {
                    if pressed {
                        self.any_pointer_button_down = true;

                        self.egui_input.events.push(egui::Event::Touch {
                            device_id: egui::TouchDeviceId(0),
                            id: egui::TouchId(0),
                            phase: egui::TouchPhase::Start,
                            pos,
                            force: 0.0,
                        });
                    } else {
                        self.any_pointer_button_down = false;

                        self.egui_input.events.push(egui::Event::PointerGone);

                        self.egui_input.events.push(egui::Event::Touch {
                            device_id: egui::TouchDeviceId(0),
                            id: egui::TouchId(0),
                            phase: egui::TouchPhase::End,
                            pos,
                            force: 0.0,
                        });
                    };
                }
            }
        }
    }

    fn on_cursor_moved(&mut self, pos_in_pixels: tao::dpi::PhysicalPosition<f64>) {
        let pos_in_points = egui::pos2(
            pos_in_pixels.x as f32 / self.pixels_per_point(),
            pos_in_pixels.y as f32 / self.pixels_per_point(),
        );
        self.pointer_pos_in_points = Some(pos_in_points);

        if self.simulate_touch_screen {
            if self.any_pointer_button_down {
                self.egui_input
                    .events
                    .push(egui::Event::PointerMoved(pos_in_points));

                self.egui_input.events.push(egui::Event::Touch {
                    device_id: egui::TouchDeviceId(0),
                    id: egui::TouchId(0),
                    phase: egui::TouchPhase::Move,
                    pos: pos_in_points,
                    force: 0.0,
                });
            }
        } else {
            self.egui_input
                .events
                .push(egui::Event::PointerMoved(pos_in_points));
        }
    }

    fn on_touch(&mut self, touch: &tao::event::Touch) {
        // Emit touch event
        self.egui_input.events.push(egui::Event::Touch {
            device_id: egui::TouchDeviceId(egui::epaint::util::hash(touch.device_id)),
            id: egui::TouchId::from(touch.id),
            phase: match touch.phase {
                TouchPhase::Started => egui::TouchPhase::Start,
                TouchPhase::Moved => egui::TouchPhase::Move,
                TouchPhase::Ended => egui::TouchPhase::End,
                TouchPhase::Cancelled => egui::TouchPhase::Cancel,
                _ => unreachable!()
            },
            pos: egui::pos2(
                touch.location.x as f32 / self.pixels_per_point(),
                touch.location.y as f32 / self.pixels_per_point(),
            ),
            force: match touch.force {
                Some(tao::event::Force::Normalized(force)) => force as f32,
                Some(tao::event::Force::Calibrated {
                    force,
                    max_possible_force,
                    ..
                }) => (force / max_possible_force) as f32,
                _ => 0_f32,
            },
        });
        // If we're not yet translating a touch or we're translating this very
        // touch …
        if self.pointer_touch_id.is_none() || self.pointer_touch_id.unwrap() == touch.id {
            // … emit PointerButton resp. PointerMoved events to emulate mouse
            match touch.phase {
                TouchPhase::Started => {
                    self.pointer_touch_id = Some(touch.id);
                    // First move the pointer to the right location
                    self.on_cursor_moved(touch.location);
                    self.on_mouse_button_input(
                        tao::event::ElementState::Pressed,
                        tao::event::MouseButton::Left,
                    );
                }
                TouchPhase::Moved => {
                    self.on_cursor_moved(touch.location);
                }
                TouchPhase::Ended => {
                    self.pointer_touch_id = None;
                    self.on_mouse_button_input(
                        tao::event::ElementState::Released,
                        tao::event::MouseButton::Left,
                    );
                    // The pointer should vanish completely to not get any
                    // hover effects
                    self.pointer_pos_in_points = None;
                    self.egui_input.events.push(egui::Event::PointerGone);
                }
                TouchPhase::Cancelled => {
                    self.pointer_touch_id = None;
                    self.pointer_pos_in_points = None;
                    self.egui_input.events.push(egui::Event::PointerGone);
                },
                _ => unreachable!()
            }
        }
    }

    fn on_mouse_wheel(&mut self, delta: tao::event::MouseScrollDelta) {
        {
            let (unit, delta) = match delta {
                tao::event::MouseScrollDelta::LineDelta(x, y) => {
                    (egui::MouseWheelUnit::Line, egui::vec2(x, y))
                }
                tao::event::MouseScrollDelta::PixelDelta(delta) => {
                    (egui::MouseWheelUnit::Point, egui::vec2(delta.x as f32, delta.y as f32) / self.pixels_per_point())
                },
                _ => unreachable!()
            };
            let modifiers = self.egui_input.modifiers;
            self.egui_input.events.push(egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
            });
        }

        let delta = match delta {
            tao::event::MouseScrollDelta::LineDelta(x, y) => {
                let points_per_scroll_line = 50.0; // Scroll speed decided by consensus: https://github.com/emilk/egui/issues/461
                egui::vec2(x, y) * points_per_scroll_line
            }
            tao::event::MouseScrollDelta::PixelDelta(delta) => {
                egui::vec2(delta.x as f32, delta.y as f32) / self.pixels_per_point()
            },
            _ => unreachable!()
        };

        if self.egui_input.modifiers.ctrl || self.egui_input.modifiers.command {
            // Treat as zoom instead:
            let factor = (delta.y / 200.0).exp();
            self.egui_input.events.push(egui::Event::Zoom(factor));
        } else if self.egui_input.modifiers.shift {
            // Treat as horizontal scrolling.
            // Note: one Mac we already get horizontal scroll events when shift is down.
            self.egui_input
                .events
                .push(egui::Event::Scroll(egui::vec2(delta.x + delta.y, 0.0)));
        } else {
            self.egui_input.events.push(egui::Event::Scroll(delta));
        }
    }

    fn on_keyboard_input(&mut self, input: &tao::event::KeyEvent) {
        let keycode = input.physical_key;
        let pressed = input.state == tao::event::ElementState::Pressed;

        if pressed {
            // VirtualKeyCode::Paste etc in winit are broken/untrustworthy,
            // so we detect these things manually:
            if is_cut_command(self.egui_input.modifiers, keycode) {
                self.egui_input.events.push(egui::Event::Cut);
            } else if is_copy_command(self.egui_input.modifiers, keycode) {
                self.egui_input.events.push(egui::Event::Copy);
            } else if is_paste_command(self.egui_input.modifiers, keycode) {
                if let Some(contents) = self.clipboard.read_text() {
                    let contents = contents.replace("\r\n", "\n");
                    if !contents.is_empty() {
                        self.egui_input.events.push(egui::Event::Paste(contents));
                    }
                }
            }
        }

        if let Some(key) = translate_virtual_key_code(keycode) {
            self.egui_input.events.push(egui::Event::Key {
                key,
                pressed,
                repeat: false, // egui will fill this in for us!
                modifiers: self.egui_input.modifiers,
            });
        }

    }

    /// Call with the output given by `egui`.
    ///
    /// This will, if needed:
    /// * update the cursor
    /// * copy text to the clipboard
    /// * open any clicked urls
    /// * update the IME
    /// *
    pub fn handle_platform_output(
        &mut self,
        window: &tao::window::Window,
        egui_ctx: &egui::Context,
        platform_output: egui::PlatformOutput,
    ) {
        let egui::PlatformOutput {
            cursor_icon,
            open_url,
            copied_text,
            events: _,                    // handled above
            mutable_text_under_cursor: _, // only used in eframe web
            text_cursor_pos,
            //#[cfg(feature = "accesskit")]
            //accesskit_update,
        } = platform_output;
        self.current_pixels_per_point = egui_ctx.pixels_per_point(); // someone can have changed it to scale the UI

        self.set_cursor_icon(window, cursor_icon);

        if let Some(open_url) = open_url {
            open_url_in_browser(&open_url.url);
        }

        if !copied_text.is_empty() {
            self.clipboard.write_text(copied_text);
        }

        if let Some(egui::Pos2 { x, y }) = text_cursor_pos {
            window.set_ime_position(tao::dpi::LogicalPosition { x, y });
        }

        //#[cfg(feature = "accesskit")]
        //if let Some(accesskit) = self.accesskit.as_ref() {
        //    if let Some(update) = accesskit_update {
        //        accesskit.update_if_active(|| update);
        //    }
        //}
    }

    fn set_cursor_icon(&mut self, window: &tao::window::Window, cursor_icon: egui::CursorIcon) {
        if self.current_cursor_icon == Some(cursor_icon) {
            // Prevent flickering near frame boundary when Windows OS tries to control cursor icon for window resizing.
            // On other platforms: just early-out to save CPU.
            return;
        }

        let is_pointer_in_window = self.pointer_pos_in_points.is_some();
        if is_pointer_in_window {
            self.current_cursor_icon = Some(cursor_icon);

            if let Some(winit_cursor_icon) = translate_cursor(cursor_icon) {
                window.set_cursor_visible(true);
                window.set_cursor_icon(winit_cursor_icon);
            } else {
                window.set_cursor_visible(false);
            }
        } else {
            // Remember to set the cursor again once the cursor returns to the screen:
            self.current_cursor_icon = None;
        }
    }
}

fn open_url_in_browser(_url: &str) {
    #[cfg(feature = "webbrowser")]
    if let Err(err) = webbrowser::open(_url) {
        log::warn!("Failed to open url: {}", err);
    }

    #[cfg(not(feature = "webbrowser"))]
    {
        log::warn!("Cannot open url - feature \"links\" not enabled.");
    }
}

/// Winit sends special keys (backspace, delete, F1, …) as characters.
/// Ignore those.
/// We also ignore '\r', '\n', '\t'.
/// Newlines are handled by the `Key::Enter` event.
fn is_printable_char(chr: char) -> bool {
    let is_in_private_use_area = '\u{e000}' <= chr && chr <= '\u{f8ff}'
        || '\u{f0000}' <= chr && chr <= '\u{ffffd}'
        || '\u{100000}' <= chr && chr <= '\u{10fffd}';

    !is_in_private_use_area && !chr.is_ascii_control()
}

fn is_cut_command(modifiers: egui::Modifiers, keycode: KeyCode) -> bool {
    (modifiers.command && keycode == KeyCode::KeyX)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == KeyCode::Delete)
}

fn is_copy_command(modifiers: egui::Modifiers, keycode: KeyCode) -> bool {
    (modifiers.command && keycode == KeyCode::KeyC)
        || (cfg!(target_os = "windows")
            && modifiers.ctrl
            && keycode == KeyCode::Insert)
}

fn is_paste_command(modifiers: egui::Modifiers, keycode: KeyCode) -> bool {
    (modifiers.command && keycode == KeyCode::KeyV)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == KeyCode::Insert)
}

fn translate_mouse_button(button: MouseButton) -> Option<egui::PointerButton> {
    match button {
        MouseButton::Left => Some(egui::PointerButton::Primary),
        MouseButton::Right => Some(egui::PointerButton::Secondary),
        MouseButton::Middle => Some(egui::PointerButton::Middle),
        MouseButton::Other(1) => Some(egui::PointerButton::Extra1),
        MouseButton::Other(2) => Some(egui::PointerButton::Extra2),
        _ => None,
    }
}

fn translate_virtual_key_code(key: KeyCode) -> Option<egui::Key> {
    use egui::Key;
    use tao::keyboard::KeyCode as VirtualKeyCode;
    Some(match key {
        VirtualKeyCode::ArrowDown => Key::ArrowDown,
        VirtualKeyCode::ArrowLeft => Key::ArrowLeft,
        VirtualKeyCode::ArrowRight => Key::ArrowRight,
        VirtualKeyCode::ArrowUp => Key::ArrowUp,

        VirtualKeyCode::Escape => Key::Escape,
        VirtualKeyCode::Tab => Key::Tab,
        VirtualKeyCode::Backspace => Key::Backspace,
        VirtualKeyCode::Enter => Key::Enter,
        VirtualKeyCode::Space => Key::Space,

        VirtualKeyCode::Insert => Key::Insert,
        VirtualKeyCode::Delete => Key::Delete,
        VirtualKeyCode::Home => Key::Home,
        VirtualKeyCode::End => Key::End,
        VirtualKeyCode::PageUp => Key::PageUp,
        VirtualKeyCode::PageDown => Key::PageDown,

        VirtualKeyCode::Minus => Key::Minus,
        // Using Mac the key with the Plus sign on it is reported as the Equals key
        // (with both English and Swedish keyboard).
        VirtualKeyCode::Equal => Key::PlusEquals,

        VirtualKeyCode::Digit0 | VirtualKeyCode::Numpad0 => Key::Num0,
        VirtualKeyCode::Digit1 | VirtualKeyCode::Numpad1 => Key::Num1,
        VirtualKeyCode::Digit2 | VirtualKeyCode::Numpad2 => Key::Num2,
        VirtualKeyCode::Digit3 | VirtualKeyCode::Numpad3 => Key::Num3,
        VirtualKeyCode::Digit4 | VirtualKeyCode::Numpad4 => Key::Num4,
        VirtualKeyCode::Digit5 | VirtualKeyCode::Numpad5 => Key::Num5,
        VirtualKeyCode::Digit6 | VirtualKeyCode::Numpad6 => Key::Num6,
        VirtualKeyCode::Digit7 | VirtualKeyCode::Numpad7 => Key::Num7,
        VirtualKeyCode::Digit8 | VirtualKeyCode::Numpad8 => Key::Num8,
        VirtualKeyCode::Digit9 | VirtualKeyCode::Numpad9 => Key::Num9,

        VirtualKeyCode::KeyA => Key::A,
        VirtualKeyCode::KeyB => Key::B,
        VirtualKeyCode::KeyC => Key::C,
        VirtualKeyCode::KeyD => Key::D,
        VirtualKeyCode::KeyE => Key::E,
        VirtualKeyCode::KeyF => Key::F,
        VirtualKeyCode::KeyG => Key::G,
        VirtualKeyCode::KeyH => Key::H,
        VirtualKeyCode::KeyI => Key::I,
        VirtualKeyCode::KeyJ => Key::J,
        VirtualKeyCode::KeyK => Key::K,
        VirtualKeyCode::KeyL => Key::L,
        VirtualKeyCode::KeyM => Key::M,
        VirtualKeyCode::KeyN => Key::N,
        VirtualKeyCode::KeyO => Key::O,
        VirtualKeyCode::KeyP => Key::P,
        VirtualKeyCode::KeyQ => Key::Q,
        VirtualKeyCode::KeyR => Key::R,
        VirtualKeyCode::KeyS => Key::S,
        VirtualKeyCode::KeyT => Key::T,
        VirtualKeyCode::KeyU => Key::U,
        VirtualKeyCode::KeyV => Key::V,
        VirtualKeyCode::KeyW => Key::W,
        VirtualKeyCode::KeyX => Key::X,
        VirtualKeyCode::KeyY => Key::Y,
        VirtualKeyCode::KeyZ => Key::Z,

        VirtualKeyCode::F1 => Key::F1,
        VirtualKeyCode::F2 => Key::F2,
        VirtualKeyCode::F3 => Key::F3,
        VirtualKeyCode::F4 => Key::F4,
        VirtualKeyCode::F5 => Key::F5,
        VirtualKeyCode::F6 => Key::F6,
        VirtualKeyCode::F7 => Key::F7,
        VirtualKeyCode::F8 => Key::F8,
        VirtualKeyCode::F9 => Key::F9,
        VirtualKeyCode::F10 => Key::F10,
        VirtualKeyCode::F11 => Key::F11,
        VirtualKeyCode::F12 => Key::F12,
        VirtualKeyCode::F13 => Key::F13,
        VirtualKeyCode::F14 => Key::F14,
        VirtualKeyCode::F15 => Key::F15,
        VirtualKeyCode::F16 => Key::F16,
        VirtualKeyCode::F17 => Key::F17,
        VirtualKeyCode::F18 => Key::F18,
        VirtualKeyCode::F19 => Key::F19,
        VirtualKeyCode::F20 => Key::F20,

        _ => {
            return None;
        }
    })
}

fn translate_cursor(cursor_icon: egui::CursorIcon) -> Option<tao::window::CursorIcon> {
    match cursor_icon {
        egui::CursorIcon::None => None,

        egui::CursorIcon::Alias => Some(tao::window::CursorIcon::Alias),
        egui::CursorIcon::AllScroll => Some(tao::window::CursorIcon::AllScroll),
        egui::CursorIcon::Cell => Some(tao::window::CursorIcon::Cell),
        egui::CursorIcon::ContextMenu => Some(tao::window::CursorIcon::ContextMenu),
        egui::CursorIcon::Copy => Some(tao::window::CursorIcon::Copy),
        egui::CursorIcon::Crosshair => Some(tao::window::CursorIcon::Crosshair),
        egui::CursorIcon::Default => Some(tao::window::CursorIcon::Default),
        egui::CursorIcon::Grab => Some(tao::window::CursorIcon::Grab),
        egui::CursorIcon::Grabbing => Some(tao::window::CursorIcon::Grabbing),
        egui::CursorIcon::Help => Some(tao::window::CursorIcon::Help),
        egui::CursorIcon::Move => Some(tao::window::CursorIcon::Move),
        egui::CursorIcon::NoDrop => Some(tao::window::CursorIcon::NoDrop),
        egui::CursorIcon::NotAllowed => Some(tao::window::CursorIcon::NotAllowed),
        egui::CursorIcon::PointingHand => Some(tao::window::CursorIcon::Hand),
        egui::CursorIcon::Progress => Some(tao::window::CursorIcon::Progress),

        egui::CursorIcon::ResizeHorizontal => Some(tao::window::CursorIcon::EwResize),
        egui::CursorIcon::ResizeNeSw => Some(tao::window::CursorIcon::NeswResize),
        egui::CursorIcon::ResizeNwSe => Some(tao::window::CursorIcon::NwseResize),
        egui::CursorIcon::ResizeVertical => Some(tao::window::CursorIcon::NsResize),

        egui::CursorIcon::ResizeEast => Some(tao::window::CursorIcon::EResize),
        egui::CursorIcon::ResizeSouthEast => Some(tao::window::CursorIcon::SeResize),
        egui::CursorIcon::ResizeSouth => Some(tao::window::CursorIcon::SResize),
        egui::CursorIcon::ResizeSouthWest => Some(tao::window::CursorIcon::SwResize),
        egui::CursorIcon::ResizeWest => Some(tao::window::CursorIcon::WResize),
        egui::CursorIcon::ResizeNorthWest => Some(tao::window::CursorIcon::NwResize),
        egui::CursorIcon::ResizeNorth => Some(tao::window::CursorIcon::NResize),
        egui::CursorIcon::ResizeNorthEast => Some(tao::window::CursorIcon::NeResize),
        egui::CursorIcon::ResizeColumn => Some(tao::window::CursorIcon::ColResize),
        egui::CursorIcon::ResizeRow => Some(tao::window::CursorIcon::RowResize),

        egui::CursorIcon::Text => Some(tao::window::CursorIcon::Text),
        egui::CursorIcon::VerticalText => Some(tao::window::CursorIcon::VerticalText),
        egui::CursorIcon::Wait => Some(tao::window::CursorIcon::Wait),
        egui::CursorIcon::ZoomIn => Some(tao::window::CursorIcon::ZoomIn),
        egui::CursorIcon::ZoomOut => Some(tao::window::CursorIcon::ZoomOut),
    }
}

// Returns a Wayland display handle if the target is running Wayland
//fn wayland_display<T>(_event_loop: &EventLoopWindowTarget<T>) -> Option<*mut c_void> {
//    #[cfg(feature = "wayland")]
//    #[cfg(any(
//        target_os = "linux",
//        target_os = "dragonfly",
//        target_os = "freebsd",
//        target_os = "netbsd",
//        target_os = "openbsd"
//    ))]
//    {
//        use tao::platform::wayland::EventLoopWindowTargetExtWayland as _;
//        return _event_loop.wayland_display();
//    }
//
//    #[allow(unreachable_code)]
//    {
//        let _ = _event_loop;
//        None
//    }
//}


// ---------------------------------------------------------------------------

/// Profiling macro for feature "puffin"
#[allow(unused_macros)]
macro_rules! profile_function {
    ($($arg: tt)*) => {
        #[cfg(feature = "puffin")]
        puffin::profile_function!($($arg)*);
    };
}

#[allow(unused_imports)]
pub(crate) use profile_function;

/// Profiling macro for feature "puffin"
#[allow(unused_macros)]
macro_rules! profile_scope {
    ($($arg: tt)*) => {
        #[cfg(feature = "puffin")]
        puffin::profile_scope!($($arg)*);
    };
}

#[allow(unused_imports)]
pub(crate) use profile_scope;
