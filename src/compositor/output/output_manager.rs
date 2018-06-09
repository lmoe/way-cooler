use compositor::{Output, Server};
use wlroots::{CompositorHandle, OutputBuilder, OutputBuilderResult, OutputManagerHandler};

pub struct OutputManager;

impl OutputManager {
    pub fn new() -> Self {
        OutputManager
    }
}

impl OutputManagerHandler for OutputManager {
    fn output_added<'output>(&mut self,
                             compositor: CompositorHandle,
                             builder: OutputBuilder<'output>)
                             -> Option<OutputBuilderResult<'output>> {
        with_handles!([(compositor: {compositor})] => {
            let server: &mut Server = compositor.into();
            let Server { ref mut cursor,
                         ref mut layout,
                         ref mut focused_output,
                         ref mut xcursor_manager,
                         .. } = *server;
            let mut res = builder.build_best_mode(Output);
            server.outputs.push(res.output.clone());
            if focused_output.is_none() {
                *focused_output = Some(res.output.clone());
            }
            with_handles!([(layout: {layout}),
                           (cursor: {cursor}),
                           (output: {&mut res.output})] => {
                layout.add_auto(output);
                cursor.attach_output_layout(layout);
                xcursor_manager.load(output.scale());
                xcursor_manager.set_cursor_image("left_ptr".to_string(), cursor);
                let (x, y) = cursor.coords();
                cursor.warp(None, x, y);
            }).expect("Could not setup output with cursor and layout");
            Some(res)
        }).unwrap()
    }
}
