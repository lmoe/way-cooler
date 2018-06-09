use compositor::{Action, Server, Shell, View};
use wlroots::{CompositorHandle, Origin, SurfaceHandle, SurfaceHandler, XdgV6ShellHandler,
              XdgV6ShellManagerHandler, XdgV6ShellState::*, XdgV6ShellSurfaceHandle};

use std::rc::Rc;
use wlroots::xdg_shell_v6_events::MoveEvent;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct XdgV6 {
    shell_surface: XdgV6ShellSurfaceHandle
}

impl XdgV6 {
    pub fn new() -> Self {
        XdgV6 { ..XdgV6::default() }
    }
}

impl XdgV6ShellHandler for XdgV6 {
    fn move_request(&mut self,
                    compositor: CompositorHandle,
                    _: SurfaceHandle,
                    shell_surface: XdgV6ShellSurfaceHandle,
                    _: &MoveEvent) {
        with_handles!([(compositor: {compositor})] => {
            let server: &mut Server = compositor.into();
            let ref mut seat = server.seat;
            let ref mut cursor = server.cursor;

            if let Some(ref mut view) = seat.focused {
                let shell: Shell = shell_surface.into();
                let action = &mut seat.action;
                if view.shell == shell {
                    with_handles!([(cursor: {cursor})] => {
                        let (lx, ly) = cursor.coords();
                        let Origin { x: shell_x, y: shell_y } = view.origin.get();
                        let (view_sx, view_sy) = (lx - shell_x as f64, ly - shell_y as f64);
                        let start = Origin::new(view_sx as _, view_sy as _);
                        *action = Some(Action::Moving { start: start });
                    }).unwrap();
                }
            }
        }).unwrap();
    }

    fn map_request(&mut self,
                   compositor: CompositorHandle,
                   _: SurfaceHandle,
                   mut shell_surface: XdgV6ShellSurfaceHandle) {
        let is_toplevel = with_handles!([(shell_surface: {&mut shell_surface})] => {
            match shell_surface.state().unwrap() {
                TopLevel(_) => true,
                _ => false
            }
        }).unwrap();

        with_handles!([(compositor: {compositor})] => {
            let server: &mut Server = compositor.into();
            let Server { ref mut seat,
                         ref mut views,
                         ref mut focused_output,
                         ref mut cursor,
                         ref mut xcursor_manager,
                         .. } = *server;
            if is_toplevel && focused_output.is_some() {
                // TODO Shouldn't this be stored in the shell, not the view?
                // Otherwise it won't decide the output until it's mapped.
                let output = focused_output.clone().unwrap();
                let view = Rc::new(View::new(Shell::XdgV6(shell_surface.into()), output));
                views.push(view.clone());
                seat.focus_view(view, views);
            }

            with_handles!([(cursor: {cursor})] => {
                seat.update_cursor_position(cursor, xcursor_manager, views, None);
            }).unwrap();
        }).unwrap();
    }

    fn unmap_request(&mut self,
                     compositor: CompositorHandle,
                     _: SurfaceHandle,
                     shell_surface: XdgV6ShellSurfaceHandle) {
        with_handles!([(compositor: {compositor})] => {
            let server: &mut Server = compositor.into();
            let Server { ref mut seat,
                         ref mut views,
                         ref mut cursor,
                         ref mut xcursor_manager,
                         .. } = *server;
            let destroyed_shell = shell_surface.into();
            if let Some(pos) = views.iter().position(|view| view.shell == destroyed_shell) {
                views.remove(pos);
            }

            if views.len() > 0 {
                seat.focus_view(views[0].clone(), views);
            } else {
                seat.clear_focus();
            }
            with_handles!([(cursor: {cursor})] => {
                seat.update_cursor_position(cursor, xcursor_manager, views, None);
            }).unwrap();
        }).unwrap();
    }
}

pub struct XdgV6ShellManager;

impl XdgV6ShellManagerHandler for XdgV6ShellManager {
    fn new_surface(&mut self,
                   _: CompositorHandle,
                   _: XdgV6ShellSurfaceHandle)
                   -> (Option<Box<XdgV6ShellHandler>>, Option<Box<SurfaceHandler>>) {
        (Some(Box::new(XdgV6::new())), None)
    }
}
