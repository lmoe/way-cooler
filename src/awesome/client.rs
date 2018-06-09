use std::fmt::{self, Display, Formatter};

use wlroots;
use rlua::{self, Lua, ToLua, UserData, Value};

use super::class::{self, Class, ClassBuilder};
use super::Screen;
use super::object::{Object, Objectable};
use compositor::{Server, View};

#[derive(Debug, Default, Clone)]
pub struct ClientState {
    view: Option<View>
}

pub struct Client<'lua>(Object<'lua>);

impl <'lua> Client<'lua> {
    fn new(lua: &Lua, view: View) -> rlua::Result<Object> {
        let class = class::class_setup(lua, "client")?;
        let res = Client::allocate(lua, class)?.build();
        let mut client = Client::cast(res)?;
        {
            let mut state = client.get_object_mut()?;
            state.view = Some(view);
        }
        Ok(client.0)
    }
}

impl Display for ClientState {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Client: {:p}", self)
    }
}

impl<'lua> ToLua<'lua> for Client<'lua> {
    fn to_lua(self, lua: &'lua Lua) -> rlua::Result<Value<'lua>> {
        self.0.to_lua(lua)
    }
}

impl UserData for ClientState {}

pub fn init(lua: &Lua) -> rlua::Result<Class> {
    method_setup(lua, Class::builder(lua, "client", None)?)?.save_class("client")?
                                                            .build()
}

fn method_setup<'lua>(lua: &'lua Lua,
                      builder: ClassBuilder<'lua>)
                      -> rlua::Result<ClassBuilder<'lua>> {
    // TODO Do properly
    use super::dummy;
    builder.method("connect_signal".into(), lua.create_function(dummy)?)?
           .method("get".into(), lua.create_function(get_client)?)
}

impl_objectable!(Client, ClientState);

fn get_client<'lua>(lua: &'lua Lua, (screen, stacked): (rlua::Value, rlua::Value))
                    -> rlua::Result<Value<'lua>> {
    let screen = match screen {
        rlua::Value::UserData(data) => {
            Some(Screen::cast(data.into())?)
        },
        _ => None
    };
    let stacked = match stacked {
        rlua::Value::Boolean(stacked) => stacked,
        _ => false
    };
    with_handles!([(compositor: {wlroots::compositor_handle().unwrap()})] => {
        let server: &mut Server = compositor.into();
        if stacked {
            // TODO Go through the stacked clients top to bottom order
            // get the first one that is on the screen _or_ just the first one if screen is None
            if screen.is_none() && server.views.len() > 0 {
                return Client::new(lua, (*server.views[0]).clone())?.to_lua(lua)
            }
            if let Some(screen) = screen {
                for view in &server.views {
                    if screen.screen()? == view.output {
                        return Client::new(lua, (**view).clone())?.to_lua(lua)
                    }
                }
            }
        } else {
            if screen.is_none() && server.views.len() > 0 {
                return Client::new(lua, (*server.views[0]).clone())?.to_lua(lua);
            }
            if let Some(screen) = screen {
                for view in &server.views {
                    if screen.screen()? == view.output {
                        return Client::new(lua, (**view).clone())?.to_lua(lua)
                    }
                }
            }
            // Just go through clients and do the same thing as above (first one on matching screen,
            // else just the first one)
        }
        lua.create_table()?.to_lua(lua)
    }).unwrap()
}
