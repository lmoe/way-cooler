mod xdg_v6;
mod layer_shell;

pub use self::xdg_v6::*;
pub use self::layer_shell::*;

use wlroots::{Area, HandleResult, SurfaceHandle, LayerSurfaceHandle, XdgV6ShellSurfaceHandle,
              Origin, Size};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Shell {
    // TODO Stable XDG
    XdgV6(XdgV6ShellSurfaceHandle),
    Layer(LayerSurfaceHandle)
}

impl Shell {
    /// Get a wlr surface from the shell.
    pub fn surface(&mut self) -> SurfaceHandle {
        match *self {
            Shell::XdgV6(ref mut shell) => {
                shell.run(|shell| shell.surface())
                     .expect("An xdg v6 client did not provide us a surface")
            },
            Shell::Layer(ref mut shell) => {
                shell.run(|shell| shell.surface())
                    .expect("Layer client did not provide us a surface")
            }
        }
    }

    /// Get the geometry of a shell.
    pub fn geometry(&mut self) -> HandleResult<Area> {
        match *self {
            Shell::XdgV6(ref mut shell) => shell.run(|shell| shell.geometry()),
            Shell::Layer(ref mut shell) => {
                shell.run(|shell| {
                    let mut area = Area::default();
                    let state = shell.current();
                    // TODO This is wrong, does this make sense even for layer shell?
                    area.origin = Origin::new(0, 0);
                    let (width, height) = state.actual_size();
                    area.size = Size::new(width as _, height as _);
                    area
                })
            }
        }
    }
}

impl Into<Shell> for XdgV6ShellSurfaceHandle {
    fn into(self) -> Shell {
        Shell::XdgV6(self)
    }
}

impl Into<Shell> for LayerSurfaceHandle {
    fn into(self) -> Shell {
        Shell::Layer(self)
    }
}
