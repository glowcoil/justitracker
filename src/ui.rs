use glium::glutin;
use rusttype::{FontCollection, Font, Scale, point, PositionedGlyph};

use render::*;

type WidgetID = usize;

pub struct UI<'a> {
    width: f32,
    height: f32,
    widgets: Vec<Widget>,
    root: WidgetID,

    mouse_x: f32,
    mouse_y: f32,
    mouse_holding: Option<WidgetID>,
    focus: Option<WidgetID>,

    font: Font<'a>,
    scale: Scale,
}

pub enum Widget {
    Empty,
    Stack(Stack),
    Button(Button),
    Textbox(Textbox),
}

pub struct Stack {
    children: Vec<WidgetID>,
}

pub struct Button {
    text: &'static str,
    on_press: Option<Box<Fn()>>,
}

impl Button {
    pub fn new(text: &'static str) -> Button {
        Button { text: text, on_press: None }
    }

    pub fn on_press<F>(&mut self, callback: F) where F: 'static + Fn() {
        self.on_press = Some(Box::new(callback));
    }
}

pub struct Textbox {
    text: String,
    on_change: Option<Box<Fn(&str)>>,
}

impl Textbox {
    pub fn new() -> Textbox {
        Textbox { text: String::new(), on_change: None }
    }

    pub fn on_change<F>(&mut self, callback: F) where F: 'static + Fn(&str) {
        self.on_change = Some(Box::new(callback));
    }
}

const PADDING: f32 = 8.0;
const SPACING: f32 = 2.0;

impl Widget {
    pub fn as_button(&mut self) -> Option<&mut Button> {
        match *self {
            Widget::Button(ref mut button) => Some(button),
            _ => None,
        }
    }

    pub fn as_textbox(&mut self) -> Option<&mut Textbox> {
        match *self {
            Widget::Textbox(ref mut textbox) => Some(textbox),
            _ => None,
        }
    }
}

impl<'a> UI<'a> {
    pub fn new(width: f32, height: f32) -> UI<'a> {
        let collection = FontCollection::from_bytes(include_bytes!("../EPKGOBLD.TTF") as &[u8]);
        let font = collection.into_font().unwrap();

        UI {
            width: width,
            height: height,
            widgets: vec![Widget::Empty],
            root: 0,

            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_holding: None,
            focus: None,

            font: font,
            scale: Scale::uniform(14.0),
        }
    }

    pub fn button(&mut self, text: &'static str) -> WidgetID {
        self.add(Widget::Button(Button::new(text)))
    }

    pub fn textbox(&mut self) -> WidgetID {
        self.add(Widget::Textbox(Textbox::new()))
    }

    pub fn stack(&mut self, ids: Vec<WidgetID>) -> WidgetID {
        self.add(Widget::Stack(Stack { children: ids }))
    }

    fn add(&mut self, widget: Widget) -> WidgetID {
        let id = self.widgets.len();
        self.widgets.push(widget);
        id
    }

    pub fn get(&self, id: WidgetID) -> &Widget {
        &self.widgets[id]
    }

    pub fn get_mut(&mut self, id: WidgetID) -> &mut Widget {
        &mut self.widgets[id]
    }

    pub fn make_root(&mut self, id: WidgetID) {
        self.root = id;
    }

    pub fn focus(&mut self, id: WidgetID) {
        self.focus = Some(id);
    }

    pub fn defocus(&mut self) {
        self.focus = None;
    }

    pub fn handle_event(&mut self, ev: glutin::Event) {
        match ev {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => {
                    self.mouse_x = x as f32;
                    self.mouse_y = y as f32;
                }
                glutin::WindowEvent::MouseInput { device_id: _, state: mouse_state, button: _, modifiers: _ } => {
                    match mouse_state {
                        glutin::ElementState::Pressed => {
                            if let Some((_widget_x, _widget_y, id)) = self.get_widget_at(self.mouse_x, self.mouse_y) {
                                match self.widgets[id] {
                                    Widget::Button { .. } | Widget::Textbox { .. } => {
                                        self.mouse_holding = Some(id);
                                    }
                                    _ => {}
                                }
                            }
                        },
                        glutin::ElementState::Released => {
                            if let Some(id) = self.mouse_holding {
                                match self.widgets[id] {
                                    Widget::Button { .. } => {
                                        if let Some((_widget_x, _widget_y, hover_id)) = self.get_widget_at(self.mouse_x, self.mouse_y) {
                                            if hover_id == id {
                                                if let Some(ref on_press) = self.widgets[id].as_button().unwrap().on_press {
                                                    on_press();
                                                }
                                            }
                                        }
                                    }
                                    Widget::Textbox { .. } => {
                                        self.focus(id);
                                    }
                                    _ => {}
                                }
                            }
                            self.mouse_holding = None;
                        },
                    }
                }
                glutin::WindowEvent::KeyboardInput { device_id: _, input } => {
                    if let Some(focus) = self.focus {
                        match self.widgets[focus] {
                            Widget::Textbox(Textbox { ref mut text, ref on_change }) => {
                                if input.state == glutin::ElementState::Pressed {
                                    if let Some(keycode) = input.virtual_keycode {
                                        if keycode == glutin::VirtualKeyCode::Back {
                                            text.pop();
                                            if let Some(ref on_change) = *on_change {
                                                on_change(text);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                glutin::WindowEvent::ReceivedCharacter(c) => {
                    if let Some(focus) = self.focus {
                        match self.widgets[focus] {
                            Widget::Textbox(Textbox { ref mut text, ref on_change }) => {
                                if !c.is_control() {
                                    text.push(c);
                                    if let Some(ref on_change) = *on_change {
                                        on_change(text);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    pub fn display(&self) -> DisplayList<'a> {
        let mut list = DisplayList {
            rects: vec![],
            glyphs: vec![],
        };

        self.display_widget(self.root, 0.0, 0.0, &mut list);

        list
    }

    fn display_widget(&self, id: WidgetID, offset_x: f32, offset_y: f32, list: &mut DisplayList<'a>) {
        match self.widgets[id] {
            Widget::Button(ref button) => {
                let (width, height) = self.get_widget_size(id);
                let width = width.unwrap();
                let height = height.unwrap();

                let color = if self.mouse_holding.is_some() && self.mouse_holding.unwrap() == id {
                    [0.02, 0.2, 0.6, 1.0]
                } else if offset_x < self.mouse_x && self.mouse_x < offset_x + width && offset_y < self.mouse_y && self.mouse_y < offset_y + height {
                    [0.3, 0.45, 0.75, 1.0]
                } else {
                    [0.15, 0.18, 0.23, 1.0]
                };

                let font = &self.font;
                let mut glyphs = layout_label(font, self.scale, offset_x + PADDING, offset_y + PADDING, button.text);

                list.rects.push(Rect { x: offset_x, y: offset_y, w: width, h: height, color: color });
                list.glyphs.append(&mut glyphs);
            }
            Widget::Textbox(Textbox { ref text, .. }) => {
                let color = [0.1, 0.15, 0.2, 1.0];

                let font = &self.font;
                let (width, height) = get_label_size(font, self.scale, text);
                let mut glyphs = layout_label(font, self.scale, offset_x + PADDING, offset_y + PADDING, text);

                list.rects.push(Rect { x: offset_x, y: offset_y, w: width.max(40.0) + 2.0 * PADDING, h: height + 2.0 * PADDING, color: color });
                list.glyphs.append(&mut glyphs);
            }
            Widget::Stack(Stack { ref children } ) => {
                let mut offset_y = offset_y;
                for widget in children.iter() {
                    self.display_widget(*widget, offset_x, offset_y, list);
                    let (_, child_height) = self.get_widget_size(*widget);
                    if let Some(child_height) = child_height {
                        offset_y += child_height + SPACING;
                    }
                }
            }
            Widget::Empty => {}
        }
    }

    fn get_widget_at(&self, x: f32, y: f32) -> Option<(f32, f32, WidgetID)> {
        let id = self.root;

        let (width, height) = self.get_widget_size(id);
        let width = width.unwrap_or(self.width);
        let height = height.unwrap_or(self.height);
        if x >= 0.0 && x < width && y >= 0.0 && y < height {
            self.get_child_widget_at(id, x, y)
        } else {
            None
        }
    }

    fn get_child_widget_at(&self, id: WidgetID, x: f32, y: f32) -> Option<(f32, f32, WidgetID)> {
        match self.widgets[id] {
            Widget::Button(..) => {
                Some((x, y, id))
            }
            Widget::Textbox(..) => {
                Some((x, y, id))
            }
            Widget::Stack(Stack { ref children }) => {
                let mut child_y = 0.0;
                for widget in children.iter() {
                    let (_, child_height) = self.get_widget_size(*widget);
                    if let Some(child_height) = child_height {
                        if y >= child_y && y < child_y + child_height {
                            return Some((x, y, *widget));
                        }
                        child_y += child_height + SPACING;
                    }
                }
                None
            }
            Widget::Empty => None
        }
    }

    fn get_widget_size(&self, id: WidgetID) -> (Option<f32>, Option<f32>) {
        match self.widgets[id] {
            Widget::Button(ref button) => {
                let (width, height) = get_label_size(&self.font, self.scale, button.text);
                (Some(width + 2.0 * PADDING), Some(height + 2.0 * PADDING))
            }
            Widget::Textbox(Textbox { ref text, .. }) => {
                let (width, height) = get_label_size(&self.font, self.scale, text);
                (Some(width.max(40.0) + 2.0 * PADDING), Some(height + 2.0 * PADDING))
            }
            Widget::Stack(Stack { ref children }) => {
                let mut width: f32 = 0.0;
                let mut height: f32 = 0.0;
                for widget in children.iter() {
                    let (child_width, child_height) = self.get_widget_size(*widget);
                    if let Some(child_width) = child_width { width = width.max(child_width); }
                    if let Some(child_height) = child_height { height += child_height; }
                }
                (Some(width), Some(height))
            }
            Widget::Empty => (None, None)
        }
    }
}

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
