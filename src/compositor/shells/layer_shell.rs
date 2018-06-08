use compositor::Server;
use wlroots::{CompositorHandle, SurfaceHandle, LayerShellHandler, LayerSurfaceHandle,
              OutputHandle, LayerShellManagerHandler };

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LayerShell {
    shell_surface: LayerSurfaceHandle,
    mapped: bool
}

impl LayerShell {
    pub fn new() -> Self {
        LayerShell { .. LayerShell::default() }
    }
}

impl LayerShellHandler for LayerShell {
    fn on_map(&mut self, _: CompositorHandle, _: SurfaceHandle, _: LayerSurfaceHandle) {
        wlr_log!(L_DEBUG, "Mapped layer surface");
        self.mapped = true;
    }

    fn on_unmap(&mut self, _: CompositorHandle, _: SurfaceHandle, _: LayerSurfaceHandle) {
        wlr_log!(L_DEBUG, "Unmapped layer surface");
        self.mapped = false;
    }

    fn destroyed(&mut self, _: CompositorHandle, _: SurfaceHandle, _: LayerSurfaceHandle) {
        wlr_log!(L_DEBUG, "Destroyed layer shell");
    }
}


pub struct LayerShellManager;

impl LayerShellManagerHandler for LayerShellManager {
    fn new_surface(&mut self,
                   compositor: CompositorHandle,
                   _: LayerSurfaceHandle,
                   output: &mut Option<OutputHandle>)
                   -> Option<Box<LayerShellHandler>> {
        wlr_log!(L_ERROR, "Output was {:?}", output);
        if output.is_none() {
            with_handles!([(compositor: {compositor})] => {
                let server: &mut Server = compositor.into();
                *output = server.outputs.first().cloned();
            }).unwrap();
        }
        Some(Box::new(LayerShell::new()))
    }
}
