use std::rc::Rc;

use glfw::{Action, Key, WindowEvent};
use gouache::*;

pub struct Context {
    pub cursor: Vec2,
    pub modifiers: glfw::Modifiers,
    pub mouse_captured: bool,
}

pub struct Rect {
    pub pos: Vec2,
    pub size: Vec2,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect { pos: Vec2::new(x, y), size: Vec2::new(width, height) }
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.pos.x && point.x < self.pos.x + self.size.x &&
        point.y >= self.pos.y && point.y < self.pos.y + self.size.y
    }
}

pub trait Component {
    fn size(&self, space: Vec2) -> Vec2;
    fn place(&mut self, rect: Rect);
    fn event(&mut self, event: glfw::WindowEvent, context: &mut Context) -> bool;
    fn draw(&self, frame: &mut Frame, context: &Context);
}

pub struct Button {
    rect: Rect,
    icon: Path,
    down: bool,
}

impl Button {
    pub fn new(icon: Path) -> Button {
        Button {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            icon,
            down: false,
        }
    }
}

impl Component for Button {
    fn size(&self, space: Vec2) -> Vec2 {
        Vec2::new(0.0, 0.0)
    }

    fn place(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn draw(&self, frame: &mut Frame, context: &Context) {
        let color = if self.down {
            Color::rgba(0.141, 0.44, 0.77, 1.0)
        } else if self.rect.contains(context.cursor) {
            Color::rgba(0.54, 0.63, 0.71, 1.0)
        } else {
            Color::rgba(0.38, 0.42, 0.48, 1.0)
        };

        frame.draw_rect(self.rect.pos, self.rect.size, Mat2x2::id(), color);
        frame.draw_path(&self.icon, self.rect.pos, Mat2x2::id(), Color::rgba(1.0, 1.0, 1.0, 1.0));
    }

    fn event(&mut self, input: glfw::WindowEvent, context: &mut Context) -> bool {
        match input {
            WindowEvent::MouseButton(glfw::MouseButton::Button1, glfw::Action::Press, _) => {
                if !context.mouse_captured && self.rect.contains(context.cursor) {
                    self.down = true;
                    context.mouse_captured = true;
                }
            }
            WindowEvent::MouseButton(glfw::MouseButton::Button1, glfw::Action::Release, _) => {
                if self.down {
                    context.mouse_captured = false;
                    self.down = false;
                    if self.rect.contains(context.cursor) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }
}

pub struct Textbox {
    rect: Rect,
    focus: bool,
    font: Rc<Font<'static>>,
    text: String,
}

impl Textbox {
    pub fn new(font: Rc<Font<'static>>) -> Textbox {
        Textbox {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            focus: false,
            font,
            text: String::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }
}

impl Component for Textbox {
    fn size(&self, space: Vec2) -> Vec2 {
        Vec2::new(0.0, 0.0)
    }

    fn place(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn draw(&self, frame: &mut Frame, context: &Context) {
        let color = if self.focus {
            Color::rgba(0.43, 0.50, 0.66, 1.0)
        } else {
            Color::rgba(0.21, 0.27, 0.32, 1.0)
        };

        frame.draw_rect(self.rect.pos, self.rect.size, Mat2x2::id(), color);
        frame.draw_text(&self.font, 14.0, &self.text, self.rect.pos, Mat2x2::id(), Color::rgba(1.0, 1.0, 1.0, 1.0));
    }

    fn event(&mut self, input: glfw::WindowEvent, context: &mut Context) -> bool {
        match input {
            WindowEvent::Char(c) => {
                self.text.push(c);
            }
            _ => {}
        }
        false
    }
}
