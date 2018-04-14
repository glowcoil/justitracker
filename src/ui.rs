use std::rc::Rc;
use std::cell::RefCell;

use glium::glutin;
use rusttype::{FontCollection, Font, Scale, point, PositionedGlyph};

use render::*;

pub struct UI {
    width: f32,
    height: f32,
    root: WidgetRef,

    input_state: InputState,
}

#[derive(Copy, Clone)]
pub enum InputEvent {
    CursorMoved { position: Point },
    MousePress { button: MouseButton },
    MouseRelease { button: MouseButton },
    MouseScroll { delta: f32 },
    KeyPress { button: KeyboardButton },
    KeyRelease { button: KeyboardButton },
    TextInput { character: char },
}

#[derive(Copy, Clone)]
pub struct InputState {
    mouse_position: Point,
    mouse_drag_origin: Option<Point>,
    mouse_left_pressed: bool,
    mouse_middle_pressed: bool,
    mouse_right_pressed: bool,
}

impl InputState {
    pub fn translate(self, delta: Point) -> InputState {
        let mut input_state = self;
        input_state.mouse_position = input_state.mouse_position + delta;
        if let Some(ref mut mouse_drag_origin) = input_state.mouse_drag_origin {
            *mouse_drag_origin = *mouse_drag_origin + delta;
        }
        input_state
    }
}

pub type WidgetRef = Rc<RefCell<Widget>>;

pub trait Widget {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState);
    fn set_container_size(&mut self, w: Option<f32>, h: Option<f32>);
    fn get_size(&self) -> (f32, f32);
    fn display(&self, input_state: InputState) -> DisplayList;
}


impl UI {
    pub fn new(width: f32, height: f32) -> UI {
        UI {
            width: width,
            height: height,
            root: Empty::new(),

            input_state: InputState {
                mouse_position: Point { x: -1.0, y: -1.0 },
                mouse_drag_origin: None,
                mouse_left_pressed: false,
                mouse_middle_pressed: false,
                mouse_right_pressed: false,
            },
        }
    }

    pub fn get_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.root.borrow_mut().set_container_size(Some(self.width), Some(self.height));
    }

    pub fn make_root(&mut self, root: WidgetRef) {
        self.root = root;
        self.root.borrow_mut().set_container_size(Some(self.width), Some(self.height));
    }

    pub fn handle_event(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::CursorMoved { position } => {
                self.input_state.mouse_position = position;
            }
            InputEvent::MousePress { button } => {
                match button {
                    MouseButton::Left => {
                        self.input_state.mouse_left_pressed = true;
                        self.input_state.mouse_drag_origin = Some(self.input_state.mouse_position);
                    }
                    MouseButton::Middle => {
                        self.input_state.mouse_middle_pressed = true;
                    }
                    MouseButton::Right => {
                        self.input_state.mouse_right_pressed = true;
                    }
                }
            }
            InputEvent::MouseRelease { button } => {
                match button {
                    MouseButton::Left => {
                        self.input_state.mouse_left_pressed = false;
                        self.input_state.mouse_drag_origin = None;
                    }
                    MouseButton::Middle => {
                        self.input_state.mouse_middle_pressed = false;
                    }
                    MouseButton::Right => {
                        self.input_state.mouse_right_pressed = false;
                    }
                }
            }
            _ => {}
        }

        self.root.borrow_mut().handle_event(ev, self.input_state);
    }

    pub fn display(&self) -> DisplayList {
        self.root.borrow().display(self.input_state)
    }
}


pub struct Empty;

impl Empty {
    pub fn new() -> Rc<RefCell<Empty>> {
        Rc::new(RefCell::new(Empty))
    }
}

impl Widget for Empty {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {}
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {}
    fn get_size(&self) -> (f32, f32) { (0.0, 0.0) }
    fn display(&self, input_state: InputState) -> DisplayList { DisplayList::new() }
}


pub struct Container {
    child: WidgetRef,
}

impl Container {
    pub fn new(child: WidgetRef) -> Rc<RefCell<Container>> {
        Rc::new(RefCell::new(Container { child: child }))
    }
}

impl Widget for Container {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        self.child.borrow_mut().handle_event(ev, input_state);
    }

    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.child.borrow_mut().set_container_size(width, height);
    }

    fn get_size(&self) -> (f32, f32) {
        self.child.borrow().get_size()
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        self.child.borrow().display(input_state)
    }
}


pub struct Column {
    children: Vec<WidgetRef>,
    focus: Option<usize>,
}

impl Column {
    pub fn new(children: Vec<WidgetRef>) -> Rc<RefCell<Column>> {
        Rc::new(RefCell::new(Column { children: children, focus: None }))
    }

    pub fn get_child(&self, i: usize) -> WidgetRef {
        self.children[i].clone()
    }
}

impl Widget for Column {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        match ev {
            InputEvent::CursorMoved { .. } | InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. } | InputEvent::MouseScroll { .. } => {
                let mouse_position = input_state.mouse_drag_origin.unwrap_or(input_state.mouse_position);

                let mut y = 0.0;
                for (i, child) in self.children.iter().enumerate() {
                    let (child_width, child_height) = child.borrow().get_size();
                    if 0.0 <= mouse_position.x && mouse_position.x < child_width && y <= mouse_position.y && mouse_position.y < y + child_height {
                        if let InputEvent::MousePress { .. } = ev {
                            self.focus = Some(i);
                        }
                        child.borrow_mut().handle_event(ev, input_state.translate(Point { x: 0.0, y: -y }));
                        break;
                    } else {
                        y += child_height;
                    }
                }
            }
            InputEvent::KeyPress { .. } | InputEvent::KeyRelease { .. } | InputEvent::TextInput { .. } => {
                if let Some(focus) = self.focus {
                    let y: f32 = self.children[0..focus].iter().map(|child| child.borrow().get_size().1).sum();
                    self.children[focus].borrow_mut().handle_event(ev, input_state.translate(Point { x: 0.0, y: -y }));
                }
            }
        }
    }

    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_size(&self) -> (f32, f32) {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in self.children.iter() {
            let (child_width, child_height) = child.borrow().get_size();
            width = width.max(child_width);
            height += child_height;
        }
        (width, height)
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let mut list = DisplayList::new();

        let mut y = 0.0;
        for child in self.children.iter() {
            let (_child_width, child_height) = child.borrow().get_size();
            let mut child_list = child.borrow().display(input_state.translate(Point { x: 0.0, y: -y }));
            child_list.translate(0.0, y);
            list.merge(child_list);
            y += child_height;
        }

        list
    }
}


pub struct Row {
    children: Vec<WidgetRef>,
    focus: Option<usize>,
}

impl Row {
    pub fn new(children: Vec<WidgetRef>) -> Rc<RefCell<Row>> {
        Rc::new(RefCell::new(Row { children: children, focus: None }))
    }

    pub fn get_child(&self, i: usize) -> WidgetRef {
        self.children[i].clone()
    }
}

impl Widget for Row {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        match ev {
            InputEvent::CursorMoved { .. } | InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. } | InputEvent::MouseScroll { .. } => {
                let mouse_position = input_state.mouse_drag_origin.unwrap_or(input_state.mouse_position);

                let mut x = 0.0;
                for (i, child) in self.children.iter().enumerate() {
                    let (child_width, child_height) = child.borrow().get_size();
                    if x <= mouse_position.x && mouse_position.x < x + child_width && 0.0 <= mouse_position.y && mouse_position.y < child_height {
                        if let InputEvent::MousePress { .. } = ev {
                            self.focus = Some(i);
                        }
                        child.borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: 0.0 }));
                        break;
                    } else {
                        x += child_width;
                    }
                }
            },
            InputEvent::KeyPress { .. } | InputEvent::KeyRelease { .. } | InputEvent::TextInput { .. } => {
                if let Some(focus) = self.focus {
                    let x: f32 = self.children[0..focus].iter().map(|child| child.borrow().get_size().0).sum();
                    self.children[focus].borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: 0.0 }));
                }
            },
        }
    }

    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_size(&self) -> (f32, f32) {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in self.children.iter() {
            let (child_width, child_height) = child.borrow().get_size();
            width += child_width;
            height = height.max(child_height);
        }
        (width, height)
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let mut list = DisplayList::new();

        let mut x = 0.0;
        for child in self.children.iter() {
            let (child_width, _child_height) = child.borrow().get_size();
            let mut child_list = child.borrow().display(input_state.translate(Point { x: -x, y: 0.0 }));
            child_list.translate(x, 0.0);
            list.merge(child_list);
            x += child_width;
        }

        list
    }
}


pub struct Button {
    contents: WidgetRef,
    on_press: Option<Box<Fn()>>,
}

impl Button {
    pub fn new(contents: WidgetRef) -> Rc<RefCell<Button>> {
        Rc::new(RefCell::new(Button { contents: contents, on_press: None }))
    }

    pub fn with_text(text: &'static str, font: Rc<Font<'static>>) -> Rc<RefCell<Button>> {
        Button::new(Label::new(text, font))
    }

    pub fn on_press<F>(&mut self, callback: F) where F: 'static + Fn() {
        self.on_press = Some(Box::new(callback));
    }
}

impl Widget for Button {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        match ev {
            InputEvent::MouseRelease { button: MouseButton::Left } => {
                if let Some(ref on_press) = self.on_press {
                    on_press();
                }
            }
            _ => {}
        }
    }

    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_size(&self) -> (f32, f32) {
        self.contents.borrow().get_size()
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let (width, height) = self.get_size();

        let mut color = [0.15, 0.18, 0.23, 1.0];
        if 0.0 <= input_state.mouse_position.x && input_state.mouse_position.x < width && 0.0 <= input_state.mouse_position.y && input_state.mouse_position.y < height {
            color = [0.3, 0.4, 0.5, 1.0];
        }
        if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
            if 0.0 <= mouse_drag_origin.x && mouse_drag_origin.x < width && 0.0 <= mouse_drag_origin.y && mouse_drag_origin.y < height {
                color = [0.02, 0.2, 0.6, 1.0];
            }
        }

        let mut list = DisplayList::new();
        list.rect(Rect { x: 0.0, y: 0.0, w: width, h: height, color: color });
        list.merge(self.contents.borrow().display(input_state));

        list
    }
}


pub struct Label {
    text: &'static str,
    font: Rc<Font<'static>>,
    scale: Scale,
}

impl Label {
    pub fn new(text: &'static str, font: Rc<Font<'static>>) -> Rc<RefCell<Label>> {
        Rc::new(RefCell::new(Label { text: text, font: font, scale: Scale::uniform(14.0) }))
    }
}

impl Widget for Label {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {

    }

    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_size(&self) -> (f32, f32) {
        get_label_size(&*self.font, self.scale, self.text)
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let glyphs = layout_label(&*self.font, self.scale, 0.0, 0.0, self.text);

        let mut list = DisplayList::new();
        for glyph in glyphs.iter() {
            list.glyph(glyph.standalone());
        }

        list
    }
}


pub struct Textbox {
    text: String,
    on_change: Option<Box<Fn(&str)>>,
    font: Rc<Font<'static>>,
    scale: Scale,
}

impl Textbox {
    pub fn new(font: Rc<Font<'static>>) -> Rc<RefCell<Textbox>> {
        Rc::new(RefCell::new(Textbox { text: String::new(), on_change: None, font: font, scale: Scale::uniform(14.0) }))
    }

    pub fn on_change<F>(&mut self, callback: F) where F: 'static + Fn(&str) {
        self.on_change = Some(Box::new(callback));
    }
}

impl Widget for Textbox {
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        match ev {
            InputEvent::KeyPress { button: KeyboardButton::Back } => {
                self.text.pop();
                if let Some(ref on_change) = self.on_change {
                    on_change(&self.text);
                }
            }
            InputEvent::TextInput { character: c } => {
                if !c.is_control() {
                    self.text.push(c);
                    if let Some(ref on_change) = self.on_change {
                        on_change(&self.text);
                    }
                }
            }
            _ => {}
        }
    }

    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_size(&self) -> (f32, f32) {
        get_label_size(&*self.font, self.scale, &self.text)
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let color = [0.1, 0.15, 0.2, 1.0];

        let (width, height) = self.get_size();
        let glyphs = layout_label(&*self.font, self.scale, 0.0, 0.0, &self.text);

        let mut list = DisplayList::new();

        list.rect(Rect { x: 0.0, y: 0.0, w: width.max(40.0), h: height, color: color });
        for glyph in glyphs.iter() {
            list.glyph(glyph.standalone());
        }

        list
    }
}


const PADDING: f32 = 8.0;
const SPACING: f32 = 2.0;

fn get_label_size<'a>(font: &'a Font,
                      scale: Scale,
                      text: &str) -> (f32, f32) {
    use unicode_normalization::UnicodeNormalization;
    let v_metrics = font.v_metrics(scale);
    let height = v_metrics.ascent - v_metrics.descent;
    let mut width = 0.0;
    let mut last_glyph_id = None;
    for c in text.nfc() {
        if c.is_control() {
            continue;
        }
        let base_glyph = if let Some(glyph) = font.glyph(c) {
            glyph
        } else {
            continue;
        };
        if let Some(id) = last_glyph_id.take() {
            width += font.pair_kerning(scale, id, base_glyph.id());
        }
        last_glyph_id = Some(base_glyph.id());
        width += base_glyph.scaled(scale).h_metrics().advance_width;
    }
    (width, height)
}

fn layout_label<'a>(font: &'a Font,
                    scale: Scale,
                    x: f32,
                    y: f32,
                    text: &str) -> Vec<PositionedGlyph<'static>> {
    use unicode_normalization::UnicodeNormalization;
    let mut result = Vec::new();
    let v_metrics = font.v_metrics(scale);
    let mut caret = point(x, y + v_metrics.ascent);
    let mut last_glyph_id = None;
    for c in text.nfc() {
        if c.is_control() {
            continue;
        }
        let base_glyph = if let Some(glyph) = font.glyph(c) {
            glyph
        } else {
            continue;
        };
        if let Some(id) = last_glyph_id.take() {
            caret.x += font.pair_kerning(scale, id, base_glyph.id());
        }
        last_glyph_id = Some(base_glyph.id());
        let glyph = base_glyph.scaled(scale).positioned(caret);
        caret.x += glyph.unpositioned().h_metrics().advance_width;
        result.push(glyph.standalone());
    }
    result
}

fn layout_paragraph<'a>(font: &'a Font,
                        scale: Scale,
                        x: f32,
                        y: f32,
                        width: u32,
                        text: &str) -> Vec<PositionedGlyph<'a>> {
    use unicode_normalization::UnicodeNormalization;
    let mut result = Vec::new();
    let v_metrics = font.v_metrics(scale);
    let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
    let mut caret = point(x, y + v_metrics.ascent);
    let mut last_glyph_id = None;
    for c in text.nfc() {
        if c.is_control() {
            match c {
                '\r' => {
                    caret = point(x, caret.y + advance_height);
                }
                '\n' => {},
                _ => {}
            }
            continue;
        }
        let base_glyph = if let Some(glyph) = font.glyph(c) {
            glyph
        } else {
            continue;
        };
        if let Some(id) = last_glyph_id.take() {
            caret.x += font.pair_kerning(scale, id, base_glyph.id());
        }
        last_glyph_id = Some(base_glyph.id());
        let mut glyph = base_glyph.scaled(scale).positioned(caret);
        if let Some(bb) = glyph.pixel_bounding_box() {
            if bb.max.x > width as i32 {
                caret = point(x, caret.y + advance_height);
                glyph = glyph.into_unpositioned().positioned(caret);
                last_glyph_id = None;
            }
        }
        caret.x += glyph.unpositioned().h_metrics().advance_width;
        result.push(glyph);
    }
    result
}

#[derive(Copy, Clone)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Copy, Clone)]
pub enum KeyboardButton {
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,
    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    AbntC1,
    AbntC2,
    Add,
    Apostrophe,
    Apps,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Decimal,
    Divide,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LMenu,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Multiply,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    OEM102,
    Period,
    PlayPause,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RMenu,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Subtract,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
}

impl KeyboardButton {
    pub fn from_glutin(keycode: glutin::VirtualKeyCode) -> KeyboardButton {
        match keycode {
            glutin::VirtualKeyCode::Key1 => KeyboardButton::Key1,
            glutin::VirtualKeyCode::Key2 => KeyboardButton::Key2,
            glutin::VirtualKeyCode::Key3 => KeyboardButton::Key3,
            glutin::VirtualKeyCode::Key4 => KeyboardButton::Key4,
            glutin::VirtualKeyCode::Key5 => KeyboardButton::Key5,
            glutin::VirtualKeyCode::Key6 => KeyboardButton::Key6,
            glutin::VirtualKeyCode::Key7 => KeyboardButton::Key7,
            glutin::VirtualKeyCode::Key8 => KeyboardButton::Key8,
            glutin::VirtualKeyCode::Key9 => KeyboardButton::Key9,
            glutin::VirtualKeyCode::Key0 => KeyboardButton::Key0,
            glutin::VirtualKeyCode::A => KeyboardButton::A,
            glutin::VirtualKeyCode::B => KeyboardButton::B,
            glutin::VirtualKeyCode::C => KeyboardButton::C,
            glutin::VirtualKeyCode::D => KeyboardButton::D,
            glutin::VirtualKeyCode::E => KeyboardButton::E,
            glutin::VirtualKeyCode::F => KeyboardButton::F,
            glutin::VirtualKeyCode::G => KeyboardButton::G,
            glutin::VirtualKeyCode::H => KeyboardButton::H,
            glutin::VirtualKeyCode::I => KeyboardButton::I,
            glutin::VirtualKeyCode::J => KeyboardButton::J,
            glutin::VirtualKeyCode::K => KeyboardButton::K,
            glutin::VirtualKeyCode::L => KeyboardButton::L,
            glutin::VirtualKeyCode::M => KeyboardButton::M,
            glutin::VirtualKeyCode::N => KeyboardButton::N,
            glutin::VirtualKeyCode::O => KeyboardButton::O,
            glutin::VirtualKeyCode::P => KeyboardButton::P,
            glutin::VirtualKeyCode::Q => KeyboardButton::Q,
            glutin::VirtualKeyCode::R => KeyboardButton::R,
            glutin::VirtualKeyCode::S => KeyboardButton::S,
            glutin::VirtualKeyCode::T => KeyboardButton::T,
            glutin::VirtualKeyCode::U => KeyboardButton::U,
            glutin::VirtualKeyCode::V => KeyboardButton::V,
            glutin::VirtualKeyCode::W => KeyboardButton::W,
            glutin::VirtualKeyCode::X => KeyboardButton::X,
            glutin::VirtualKeyCode::Y => KeyboardButton::Y,
            glutin::VirtualKeyCode::Z => KeyboardButton::Z,
            glutin::VirtualKeyCode::Escape => KeyboardButton::Escape,
            glutin::VirtualKeyCode::F1 => KeyboardButton::F1,
            glutin::VirtualKeyCode::F2 => KeyboardButton::F2,
            glutin::VirtualKeyCode::F3 => KeyboardButton::F3,
            glutin::VirtualKeyCode::F4 => KeyboardButton::F4,
            glutin::VirtualKeyCode::F5 => KeyboardButton::F5,
            glutin::VirtualKeyCode::F6 => KeyboardButton::F6,
            glutin::VirtualKeyCode::F7 => KeyboardButton::F7,
            glutin::VirtualKeyCode::F8 => KeyboardButton::F8,
            glutin::VirtualKeyCode::F9 => KeyboardButton::F9,
            glutin::VirtualKeyCode::F10 => KeyboardButton::F10,
            glutin::VirtualKeyCode::F11 => KeyboardButton::F11,
            glutin::VirtualKeyCode::F12 => KeyboardButton::F12,
            glutin::VirtualKeyCode::F13 => KeyboardButton::F13,
            glutin::VirtualKeyCode::F14 => KeyboardButton::F14,
            glutin::VirtualKeyCode::F15 => KeyboardButton::F15,
            glutin::VirtualKeyCode::Snapshot => KeyboardButton::Snapshot,
            glutin::VirtualKeyCode::Scroll => KeyboardButton::Scroll,
            glutin::VirtualKeyCode::Pause => KeyboardButton::Pause,
            glutin::VirtualKeyCode::Insert => KeyboardButton::Insert,
            glutin::VirtualKeyCode::Home => KeyboardButton::Home,
            glutin::VirtualKeyCode::Delete => KeyboardButton::Delete,
            glutin::VirtualKeyCode::End => KeyboardButton::End,
            glutin::VirtualKeyCode::PageDown => KeyboardButton::PageDown,
            glutin::VirtualKeyCode::PageUp => KeyboardButton::PageUp,
            glutin::VirtualKeyCode::Left => KeyboardButton::Left,
            glutin::VirtualKeyCode::Up => KeyboardButton::Up,
            glutin::VirtualKeyCode::Right => KeyboardButton::Right,
            glutin::VirtualKeyCode::Down => KeyboardButton::Down,
            glutin::VirtualKeyCode::Back => KeyboardButton::Back,
            glutin::VirtualKeyCode::Return => KeyboardButton::Return,
            glutin::VirtualKeyCode::Space => KeyboardButton::Space,
            glutin::VirtualKeyCode::Compose => KeyboardButton::Compose,
            glutin::VirtualKeyCode::Numlock => KeyboardButton::Numlock,
            glutin::VirtualKeyCode::Numpad0 => KeyboardButton::Numpad0,
            glutin::VirtualKeyCode::Numpad1 => KeyboardButton::Numpad1,
            glutin::VirtualKeyCode::Numpad2 => KeyboardButton::Numpad2,
            glutin::VirtualKeyCode::Numpad3 => KeyboardButton::Numpad3,
            glutin::VirtualKeyCode::Numpad4 => KeyboardButton::Numpad4,
            glutin::VirtualKeyCode::Numpad5 => KeyboardButton::Numpad5,
            glutin::VirtualKeyCode::Numpad6 => KeyboardButton::Numpad6,
            glutin::VirtualKeyCode::Numpad7 => KeyboardButton::Numpad7,
            glutin::VirtualKeyCode::Numpad8 => KeyboardButton::Numpad8,
            glutin::VirtualKeyCode::Numpad9 => KeyboardButton::Numpad9,
            glutin::VirtualKeyCode::AbntC1 => KeyboardButton::AbntC1,
            glutin::VirtualKeyCode::AbntC2 => KeyboardButton::AbntC2,
            glutin::VirtualKeyCode::Add => KeyboardButton::Add,
            glutin::VirtualKeyCode::Apostrophe => KeyboardButton::Apostrophe,
            glutin::VirtualKeyCode::Apps => KeyboardButton::Apps,
            glutin::VirtualKeyCode::At => KeyboardButton::At,
            glutin::VirtualKeyCode::Ax => KeyboardButton::Ax,
            glutin::VirtualKeyCode::Backslash => KeyboardButton::Backslash,
            glutin::VirtualKeyCode::Calculator => KeyboardButton::Calculator,
            glutin::VirtualKeyCode::Capital => KeyboardButton::Capital,
            glutin::VirtualKeyCode::Colon => KeyboardButton::Colon,
            glutin::VirtualKeyCode::Comma => KeyboardButton::Comma,
            glutin::VirtualKeyCode::Convert => KeyboardButton::Convert,
            glutin::VirtualKeyCode::Decimal => KeyboardButton::Decimal,
            glutin::VirtualKeyCode::Divide => KeyboardButton::Divide,
            glutin::VirtualKeyCode::Equals => KeyboardButton::Equals,
            glutin::VirtualKeyCode::Grave => KeyboardButton::Grave,
            glutin::VirtualKeyCode::Kana => KeyboardButton::Kana,
            glutin::VirtualKeyCode::Kanji => KeyboardButton::Kanji,
            glutin::VirtualKeyCode::LAlt => KeyboardButton::LAlt,
            glutin::VirtualKeyCode::LBracket => KeyboardButton::LBracket,
            glutin::VirtualKeyCode::LControl => KeyboardButton::LControl,
            glutin::VirtualKeyCode::LMenu => KeyboardButton::LMenu,
            glutin::VirtualKeyCode::LShift => KeyboardButton::LShift,
            glutin::VirtualKeyCode::LWin => KeyboardButton::LWin,
            glutin::VirtualKeyCode::Mail => KeyboardButton::Mail,
            glutin::VirtualKeyCode::MediaSelect => KeyboardButton::MediaSelect,
            glutin::VirtualKeyCode::MediaStop => KeyboardButton::MediaStop,
            glutin::VirtualKeyCode::Minus => KeyboardButton::Minus,
            glutin::VirtualKeyCode::Multiply => KeyboardButton::Multiply,
            glutin::VirtualKeyCode::Mute => KeyboardButton::Mute,
            glutin::VirtualKeyCode::MyComputer => KeyboardButton::MyComputer,
            glutin::VirtualKeyCode::NavigateForward => KeyboardButton::NavigateForward,
            glutin::VirtualKeyCode::NavigateBackward => KeyboardButton::NavigateBackward,
            glutin::VirtualKeyCode::NextTrack => KeyboardButton::NextTrack,
            glutin::VirtualKeyCode::NoConvert => KeyboardButton::NoConvert,
            glutin::VirtualKeyCode::NumpadComma => KeyboardButton::NumpadComma,
            glutin::VirtualKeyCode::NumpadEnter => KeyboardButton::NumpadEnter,
            glutin::VirtualKeyCode::NumpadEquals => KeyboardButton::NumpadEquals,
            glutin::VirtualKeyCode::OEM102 => KeyboardButton::OEM102,
            glutin::VirtualKeyCode::Period => KeyboardButton::Period,
            glutin::VirtualKeyCode::PlayPause => KeyboardButton::PlayPause,
            glutin::VirtualKeyCode::Power => KeyboardButton::Power,
            glutin::VirtualKeyCode::PrevTrack => KeyboardButton::PrevTrack,
            glutin::VirtualKeyCode::RAlt => KeyboardButton::RAlt,
            glutin::VirtualKeyCode::RBracket => KeyboardButton::RBracket,
            glutin::VirtualKeyCode::RControl => KeyboardButton::RControl,
            glutin::VirtualKeyCode::RMenu => KeyboardButton::RMenu,
            glutin::VirtualKeyCode::RShift => KeyboardButton::RShift,
            glutin::VirtualKeyCode::RWin => KeyboardButton::RWin,
            glutin::VirtualKeyCode::Semicolon => KeyboardButton::Semicolon,
            glutin::VirtualKeyCode::Slash => KeyboardButton::Slash,
            glutin::VirtualKeyCode::Sleep => KeyboardButton::Sleep,
            glutin::VirtualKeyCode::Stop => KeyboardButton::Stop,
            glutin::VirtualKeyCode::Subtract => KeyboardButton::Subtract,
            glutin::VirtualKeyCode::Sysrq => KeyboardButton::Sysrq,
            glutin::VirtualKeyCode::Tab => KeyboardButton::Tab,
            glutin::VirtualKeyCode::Underline => KeyboardButton::Underline,
            glutin::VirtualKeyCode::Unlabeled => KeyboardButton::Unlabeled,
            glutin::VirtualKeyCode::VolumeDown => KeyboardButton::VolumeDown,
            glutin::VirtualKeyCode::VolumeUp => KeyboardButton::VolumeUp,
            glutin::VirtualKeyCode::Wake => KeyboardButton::Wake,
            glutin::VirtualKeyCode::WebBack => KeyboardButton::WebBack,
            glutin::VirtualKeyCode::WebFavorites => KeyboardButton::WebFavorites,
            glutin::VirtualKeyCode::WebForward => KeyboardButton::WebForward,
            glutin::VirtualKeyCode::WebHome => KeyboardButton::WebHome,
            glutin::VirtualKeyCode::WebRefresh => KeyboardButton::WebRefresh,
            glutin::VirtualKeyCode::WebSearch => KeyboardButton::WebSearch,
            glutin::VirtualKeyCode::WebStop => KeyboardButton::WebStop,
            glutin::VirtualKeyCode::Yen => KeyboardButton::Yen,
        }
    }
}

use std::ops;

#[derive(Copy, Clone)]
pub struct Point { pub x: f32, pub y: f32 }

impl ops::Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl ops::Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}

impl ops::Mul<f32> for Point {
    type Output = Point;
    fn mul(self, rhs: f32) -> Point {
        Point { x: self.x * rhs, y: self.y * rhs }
    }
}

impl ops::Mul<Point> for f32 {
    type Output = Point;
    fn mul(self, rhs: Point) -> Point {
        Point { x: self * rhs.x, y: self * rhs.y }
    }
}
