use compositor::{Server, Shell, View};
use std::rc::Rc;
use std::time::Duration;
use wlroots::events::seat_events::SetCursorEvent;
use wlroots::pointer_events::ButtonEvent;
use wlroots::utils::{current_time, L_DEBUG};
use wlroots::{CompositorHandle, Cursor, Origin, SeatHandle, SeatHandler, SurfaceHandle,
              XCursorManager, SurfaceHandler, DragIconHandler, DragIconHandle};

#[derive(Debug, Default)]
pub struct SeatManager;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Action {
    /// We are moving a view.
    ///
    /// The start is the surface level coordinates of where the first click was
    Moving { start: Origin }
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Seat {
    pub seat: SeatHandle,
    pub focused: Option<Rc<View>>,
    pub action: Option<Action>,
    pub has_client_cursor: bool,
    pub meta: bool
}

impl Seat {
    pub fn new(seat: SeatHandle) -> Seat {
        Seat { seat,
               meta: false,
               ..Seat::default() }
    }

    pub fn clear_focus(&mut self) {
        if let Some(focused_view) = self.focused.take() {
            focused_view.activate(false);
        }
        with_handles!([(seat: {&mut self.seat})] => {
            seat.keyboard_clear_focus();
        }).unwrap();
    }

    pub fn focus_view(&mut self, view: Rc<View>, views: &mut Vec<Rc<View>>) {
        if let Some(ref focused) = self.focused {
            if *focused == view {
                return
            }
            focused.activate(false);
        }
        self.focused = Some(view.clone());
        view.activate(true);

        if let Some(idx) = views.iter().position(|v| *v == view) {
            let v = views.remove(idx);
            views.insert(0, v);
        }

        with_handles!([(seat: {&mut self.seat})] => {
            if let Some(keyboard) = seat.get_keyboard() {
                with_handles!([(keyboard: {keyboard}), (surface: {view.surface()})] => {
                    seat.keyboard_notify_enter(surface,
                                               &mut keyboard.keycodes(),
                                               &mut keyboard.get_modifier_masks());
                }).unwrap();
            }
        }).unwrap();
    }

    pub fn send_button(&self, event: &ButtonEvent) {
        with_handles!([(seat: {&self.seat})] => {
            seat.pointer_notify_button(Duration::from_millis(event.time_msec() as _),
            event.button(),
            event.state() as u32);
        }).unwrap();
    }

    pub fn move_view<O>(&mut self, cursor: &mut Cursor, view: &View, start: O)
        where O: Into<Option<Origin>>
    {
        let Origin { x: shell_x,
                     y: shell_y } = view.origin.get();
        let (lx, ly) = cursor.coords();
        match start.into() {
            None => {
                let (view_sx, view_sy) = (lx - shell_x as f64, ly - shell_y as f64);
                let start = Origin::new(view_sx as _, view_sy as _);
                self.action = Some(Action::Moving { start });
            }
            Some(start) => {
                let pos = Origin::new(lx as i32 - start.x, ly as i32 - start.y);
                view.origin.replace(pos);
            }
        };
    }

    pub fn view_at_pointer(views: &mut [Rc<View>],
                           cursor: &mut Cursor)
                           -> (Option<Rc<View>>, Option<SurfaceHandle>, f64, f64) {
        for view in views {
            match view.shell {
                Shell::XdgV6(ref shell) => {
                    let (mut sx, mut sy) = (0.0, 0.0);
                    let surface = with_handles!([(shell: {shell})] => {
                        let (lx, ly) = cursor.coords();
                        let Origin {x: shell_x, y: shell_y} = view.origin.get();
                        let (view_sx, view_sy) = (lx - shell_x as f64, ly - shell_y as f64);
                        shell.surface_at(view_sx, view_sy, &mut sx, &mut sy)
                    }).unwrap();
                    if surface.is_some() {
                        return (Some(view.clone()), surface, sx, sy)
                    }
                }
            }
        }
        (None, None, 0.0, 0.0)
    }

    pub fn update_cursor_position(&mut self,
                                  cursor: &mut Cursor,
                                  xcursor_manager: &mut XCursorManager,
                                  views: &mut [Rc<View>],
                                  time_msec: Option<u32>) {
        let time = if let Some(time_msec) = time_msec {
            Duration::from_millis(time_msec as u64)
        } else {
            current_time()
        };

        match self.action {
            Some(Action::Moving { start }) => {
                self.focused = self.focused.take().map(|f| {
                                                           self.move_view(cursor, &f, start);
                                                           f
                                                       });
            }
            _ => {
                let (_view, surface, sx, sy) = Seat::view_at_pointer(views, cursor);
                match surface {
                    Some(surface) => {
                        with_handles!([(surface: {surface}), (seat: {&mut self.seat})] => {
                            seat.pointer_notify_enter(surface, sx, sy);
                            seat.pointer_notify_motion(time, sx, sy);
                        }).unwrap();
                    }
                    None => {
                        if self.has_client_cursor {
                            xcursor_manager.set_cursor_image("left_ptr".to_string(), cursor);
                            self.has_client_cursor = false;
                        }
                        with_handles!([(seat: {&mut self.seat})] => {
                            seat.pointer_clear_focus();
                        }).unwrap();
                    }
                }
            }
        }
    }
}

struct WCDragIconHandler;

impl DragIconHandler for WCDragIconHandler {
    fn on_map(&mut self, compositor: CompositorHandle, drag_icon: DragIconHandle) {
        wlr_log!(L_DEBUG, "TODO: handle drag icon mapped");
    }

    fn on_unmap(&mut self, compositor: CompositorHandle, drag_icon: DragIconHandle) {
        wlr_log!(L_DEBUG, "TODO: handle drag icon unmapped");
    }

    fn destroyed(&mut self, compositor: CompositorHandle, drag_icon: DragIconHandle) {
        wlr_log!(L_DEBUG, "TODO: handle drag icon destroyed");
    }
}

impl SeatHandler for SeatManager {
    fn cursor_set(&mut self, compositor: CompositorHandle, _: SeatHandle, event: &SetCursorEvent) {
        if let Some(surface) = event.surface() {
            with_handles!([(compositor: {compositor}), (surface: {surface})] => {
                let server: &mut Server = compositor.into();
                let Server { ref mut cursor,
                             ref mut seat,
                .. } = *server;
                with_handles!([(cursor: {&mut *cursor})] => {
                    let (hotspot_x, hotspot_y) = event.location();
                    let surface = &*surface;
                    cursor.set_surface(Some(surface), hotspot_x, hotspot_y);
                    seat.has_client_cursor = true;
                }).unwrap();
            }).unwrap();
        }
    }

    fn new_drag_icon(&mut self,
                     compositor: CompositorHandle,
                     seat: SeatHandle,
                     drag_icon: DragIconHandle)
                     -> (Option<Box<DragIconHandler>>, Option<Box<SurfaceHandler>>) {
        (Some(Box::new(WCDragIconHandler)), None)
    }
}

impl SeatManager {
    pub fn new() -> Self {
        SeatManager::default()
    }
}
