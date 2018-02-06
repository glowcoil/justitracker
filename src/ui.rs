use glium::glutin;

use render::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState { Up, Hover, Down }

type WidgetID = usize;

pub struct UI {
    mouse_x: f32,
    mouse_y: f32,
    widgets: Vec<Widget>,
}

pub enum Widget {
    HList(Vec<WidgetID>),
    VList(Vec<WidgetID>),
    ScrollBox { w: f32, h: f32, contents: WidgetID },
    Button {
        x: f32,
        y: f32,
        text: &'static str,
        state: ButtonState,
    },
}

impl UI {
    pub fn new() -> UI {
        UI {
            mouse_x: 0.0,
            mouse_y: 0.0,
            widgets: vec![],
        }
    }

    pub fn add(&mut self, widget: Widget) -> WidgetID {
        let id = self.widgets.len();
        self.widgets.push(widget);
        id
    }

    pub fn handle_event(&mut self, ev: glutin::Event) -> glutin::ControlFlow {
        match ev {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::Closed => return glutin::ControlFlow::Break,
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => {
                    self.mouse_x = x as f32;
                    self.mouse_y = y as f32;
                    if let Widget::Button { x: button_x, y: button_y, text, ref mut state } = self.widgets[0] {
                        if 10.0 <= self.mouse_x && self.mouse_x < 10.0 + 60.0 && 10.0 <= self.mouse_y && self.mouse_y < 10.0 + 20.0 {
                            *state = ButtonState::Hover;
                        } else {
                            *state = ButtonState::Up;
                        }
                    }
                },
                glutin::WindowEvent::MouseInput { device_id, state: mouse_state, button } => {
                    if let Widget::Button { x: button_x, y: button_y, text, ref mut state } = self.widgets[0] {
                        if 10.0 <= self.mouse_x && self.mouse_x < 10.0 + 60.0 && 10.0 <= self.mouse_y && self.mouse_y < 10.0 + 20.0 {
                            *state = match mouse_state {
                                glutin::ElementState::Pressed => ButtonState::Down,
                                glutin::ElementState::Released => ButtonState::Hover,
                            };
                        } else {
                            *state = ButtonState::Up;
                        }
                    }
                },
                _ => (),
            },
            _ => (),
        }

        glutin::ControlFlow::Continue
    }

    pub fn display(&self) -> DisplayList {
        let mut list = DisplayList {
            rects: vec![],
            texts: vec![],
        };

        for widget in &self.widgets {
            match *widget {
                Widget::Button { x, y, text, state } => {
                    let color = match state {
                        ButtonState::Up => [0.1, 0.3, 0.8, 1.0],
                        ButtonState::Hover => [0.3, 0.4, 1.0, 1.0],
                        ButtonState::Down => [0.1, 0.2, 0.4, 1.0],
                    };

                    list.rects.push(Rect { x: x, y: y, w: 60.0, h: 20.0, color: color });
                    list.texts.push(Text{ text: text.to_string(), x: x + 4.0, y: y + 4.0 });
                }
                _ => {}
            }
        }

        list
    }
}
