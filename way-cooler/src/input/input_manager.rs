use wlroots::{Capability, CompositorHandle, InputManagerHandler, KeyboardHandle, KeyboardHandler,
              PointerHandle, PointerHandler};
use wlroots::wlroots_dehandle;

pub struct InputManager;

impl InputManager {
    pub fn new() -> Self {
        InputManager
    }
}

impl InputManagerHandler for InputManager {
    #[wlroots_dehandle(compositor, keyboard, seat)]
    fn keyboard_added(&mut self,
                      compositor: CompositorHandle,
                      keyboard: KeyboardHandle)
                      -> Option<Box<KeyboardHandler>> {
        {
            use compositor as compositor;
            use keyboard as keyboard;
            let server: &mut ::Server = compositor.into();
            server.keyboards.push(keyboard.weak_reference());
            // Now that we have at least one keyboard, update the seat capabilities.
            let seat_handle = &server.seat.seat;
            use seat_handle as seat;
            let mut capabilities = seat.capabilities();
            capabilities.insert(Capability::Keyboard);
            seat.set_capabilities(capabilities);
            seat.set_keyboard(keyboard.input_device());
        }
        Some(Box::new(::Keyboard))
    }

    #[wlroots_dehandle(compositor, pointer, cursor, seat)]
    fn pointer_added(&mut self,
                     compositor: CompositorHandle,
                     pointer: PointerHandle)
                     -> Option<Box<PointerHandler>> {
        {
            use compositor as compositor;
            use pointer as pointer;
            let server: &mut ::Server = compositor.into();
            server.pointers.push(pointer.weak_reference());
            if server.pointers.len() == 1 {
                // Now that we have at least one keyboard, update the seat capabilities.
                let seat_handle = &server.seat.seat;
                use seat_handle as seat;
                let mut capabilities = seat.capabilities();
                capabilities.insert(Capability::Pointer);
                seat.set_capabilities(capabilities);
            };

            let cursor_handle = &server.cursor;
            use cursor_handle as cursor;
            cursor.attach_input_device(pointer.input_device());
        }
        Some(Box::new(::Pointer))
    }
}
