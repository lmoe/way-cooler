use std::{collections::HashSet, rc::Rc, time::Duration};
use wlroots::{self,
              events::seat_events::SetCursorEvent,
              pointer_events::ButtonEvent,
              utils::{current_time, Edges},
              Area, CompositorHandle, Cursor, CursorHandle, DragIconHandle, Origin, SeatHandle,
              SeatHandler, Size, SurfaceHandle, SurfaceHandler, XCursorManager};

#[derive(Debug, Default)]
pub struct SeatManager;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Action {
    /// We are moving a view.
    ///
    /// The start is the surface level coordinates of where the first click was
    Moving {
        start: Origin
    },
    Resizing {
        start: Origin,
        offset: Origin,
        original_size: Size,
        edges: Edges
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct DragIcon {
    pub handle: DragIconHandle
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Seat {
    pub seat: SeatHandle,
    pub focused: Option<Rc<::View>>,
    pub action: Option<Action>,
    pub has_client_cursor: bool,
    pub meta: bool,
    pub drag_icons: HashSet<DragIcon>
}

impl Seat {
    pub fn new(seat: SeatHandle) -> Seat { Seat { seat, meta: false, ..Seat::default() } }

    pub fn clear_focus(&mut self) {
        if let Some(focused_view) = self.focused.take() {
            focused_view.activate(false);
        }
        dehandle!(@seat = {&self.seat};
                  seat.keyboard_clear_focus())
    }

    pub fn focus_view(&mut self, view: Rc<::View>, views: &mut Vec<Rc<::View>>) {
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

        dehandle!(
            @seat = {&self.seat};
            if let Some(keyboard) = seat.get_keyboard() {
                with_handles!([(keyboard: {keyboard}), (surface: {view.surface()})] => {
                    seat.keyboard_notify_enter(surface,
                                               &mut keyboard.keycodes(),
                                               &mut keyboard.get_modifier_masks());
                }).unwrap();
            }
        );
    }

    pub fn send_button(&self, event: &ButtonEvent) {
        dehandle!(
            @seat = {&self.seat};
            seat.pointer_notify_button(Duration::from_millis(event.time_msec() as _),
            event.button(),
            event.state() as u32));
    }

    pub fn move_view<O>(&mut self, cursor: &mut Cursor, view: &::View, start: O)
        where O: Into<Option<Origin>> {
        let Origin { x: shell_x, y: shell_y } = view.origin.get();
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

    pub fn begin_resize(&mut self,
                        cursor: &mut CursorHandle,
                        view: Rc<::View>,
                        views: &mut Vec<Rc<::View>>,
                        edges: Edges)
    {
        self.focus_view(view.clone(), views);
        with_handles!([(cursor: {cursor})] => {
            let Origin { x: view_x, y: view_y } = view.origin.get();
            let (lx, ly) = cursor.coords();
            let (view_sx, view_sy) = (lx - view_x as f64, ly - view_y as f64);
            let offset = Origin::new(view_sx as _, view_sy as _);
            self.action = Some(Action::Resizing {
                start: Origin { x: view_x, y: view_y },
                offset,
                original_size: view.get_size(),
                edges
            });
        }).unwrap();
    }

    pub fn view_at_pointer(views: &mut [Rc<::View>],
                           cursor: &mut Cursor)
                           -> (Option<Rc<::View>>, Option<SurfaceHandle>, f64, f64)
    {
        for view in views {
            match view.shell.clone() {
                ::Shell::XdgV6(mut shell) => {
                    let (mut sx, mut sy) = (0.0, 0.0);
                    let surface = dehandle!(
                        @shell = {shell};
                        let (lx, ly) = cursor.coords();
                        let Origin {x: shell_x, y: shell_y} = view.origin.get();
                        let (view_sx, view_sy) = (lx - shell_x as f64, ly - shell_y as f64);
                        shell.surface_at(view_sx, view_sy, &mut sx, &mut sy)
                    );
                    if surface.is_some() {
                        return (Some(view.clone()), surface, sx, sy)
                    }
                }
                ::Shell::Xdg(mut shell) => {
                    let (mut sx, mut sy) = (0.0, 0.0);
                    let surface = dehandle!(
                        @shell = {shell};
                        let (lx, ly) = cursor.coords();
                        let Origin {x: shell_x, y: shell_y} = view.origin.get();
                        let (view_sx, view_sy) = (lx - shell_x as f64, ly - shell_y as f64);
                        shell.surface_at(view_sx, view_sy, &mut sx, &mut sy)
                    );
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
                                  views: &mut [Rc<::View>],
                                  time_msec: Option<u32>)
    {
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
            Some(Action::Resizing { offset, start, original_size, edges }) => {
                self.focused =
                    self.focused.take().map(|view| {
                                           let (cursor_lx, cursor_ly) = cursor.coords();
                                           let Origin { x: offs_x, y: offs_y } = offset;
                                           let Origin { x: mut view_x, y: mut view_y } = start;
                                           let (dx, dy) = (cursor_lx as i32 - offs_x - view_x,
                                                           cursor_ly as i32 - offs_y - view_y);
                                           let Size { mut width, mut height } = original_size;

                                           if edges.contains(Edges::WLR_EDGE_BOTTOM) {
                                               height += dy;
                                           } else if edges.contains(Edges::WLR_EDGE_TOP) {
                                               view_y += dy;
                                               height -= dy;
                                           }

                                           if edges.contains(Edges::WLR_EDGE_LEFT) {
                                               view_x += dx;
                                               width -= dx;
                                           } else if edges.contains(Edges::WLR_EDGE_RIGHT) {
                                               width += dx;
                                           }

                                           view.move_resize(Area { origin: Origin { x:
                                                                                        view_x,
                                                                                    y:
                                                                                        view_y },
                                                                   size: Size { width, height } });
                                           view
                                       });
            }
            _ => {
                let (_view, surface, sx, sy) = Seat::view_at_pointer(views, cursor);
                match surface {
                    Some(surface) => {
                        dehandle!(
                            @surface = {surface};
                            @seat = {&self.seat};
                            seat.pointer_notify_enter(surface, sx, sy);
                            seat.pointer_notify_motion(time, sx, sy)
                        );
                    }
                    None => {
                        if self.has_client_cursor {
                            xcursor_manager.set_cursor_image("left_ptr".to_string(), cursor);
                            self.has_client_cursor = false;
                        }
                        dehandle!(
                            @seat = {&self.seat};
                            seat.pointer_clear_focus()
                        );
                    }
                }
            }
        }
    }
}

struct DragIconHandler;

impl wlroots::DragIconHandler for DragIconHandler {
    fn on_map(&mut self, _: CompositorHandle, _: DragIconHandle) {
        // TODO damage the drag icon surface location
    }

    fn on_unmap(&mut self, _: CompositorHandle, _: DragIconHandle) {
        // TODO damage the drag icon surface location
    }

    fn destroyed(&mut self, compositor: CompositorHandle, drag_icon: DragIconHandle) {
        with_handles!([(compositor: {compositor})] => {
            let server: &mut ::Server = compositor.into();
            server.seat.drag_icons.remove(&DragIcon{ handle: drag_icon });
        }).unwrap();
    }
}

impl SeatHandler for SeatManager {
    fn cursor_set(&mut self, compositor: CompositorHandle, _: SeatHandle, event: &SetCursorEvent) {
        if let Some(surface) = event.surface() {
            dehandle!(
                @compositor = {compositor};
                @surface = {surface};
                let server: &mut ::Server = compositor.into();
                let ::Server { ref cursor,
                             ref mut seat,
                .. } = *server;
                @cursor = {cursor};
                let (hotspot_x, hotspot_y) = event.location();
                let surface = &*surface;
                cursor.set_surface(Some(surface), hotspot_x, hotspot_y);
                seat.has_client_cursor = true
            );
        }
    }

    fn new_drag_icon(&mut self,
                     compositor: CompositorHandle,
                     _: SeatHandle,
                     drag_icon: DragIconHandle)
                     -> (Option<Box<wlroots::DragIconHandler>>, Option<Box<SurfaceHandler>>)
    {
        with_handles!([(compositor: {compositor})] => {
            let server: &mut ::Server = compositor.into();
            let ::Server { ref mut seat, .. } = *server;
            seat.drag_icons.insert(DragIcon { handle: drag_icon });
        }).unwrap();
        (Some(Box::new(DragIconHandler)), None)
    }
}

impl SeatManager {
    pub fn new() -> Self { SeatManager::default() }
}
