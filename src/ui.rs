use std::rc::Rc;
use std::cell::RefCell;

use glium::glutin;
use rusttype::{Font, Scale, point, PositionedGlyph};

use render::*;

pub struct UI {
    width: f32,
    height: f32,
    root: WidgetRef,

    input_state: InputState,
    keyboard_focus: Option<WidgetRef>,
    mouse_focus: Option<WidgetRef>,
    mouse_position_captured: bool,
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
    LostKeyboardFocus,
}

#[derive(Copy, Clone)]
pub struct UIEventResponse {
    pub set_mouse_position: Option<(f32, f32)>,
    pub mouse_cursor: MouseCursor,
}

impl Default for UIEventResponse {
    fn default() -> UIEventResponse {
        UIEventResponse {
            set_mouse_position: None,
            mouse_cursor: MouseCursor::Default,
        }
    }
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

pub struct EventResponse {
    responder: Option<WidgetRef>,
    capture_keyboard: bool,
    capture_mouse: bool,
    capture_mouse_position: bool,
    mouse_cursor: MouseCursor,
}

impl Default for EventResponse {
    fn default() -> EventResponse {
        EventResponse {
            responder: None,
            capture_keyboard: false,
            capture_mouse: false,
            capture_mouse_position: false,
            mouse_cursor: MouseCursor::Default,
        }
    }
}

pub type WidgetRef = Rc<RefCell<Widget>>;

pub trait Widget {
    fn set_container_size(&mut self, w: Option<f32>, h: Option<f32>);
    fn get_min_size(&self) -> (f32, f32);
    fn get_size(&self) -> (f32, f32);
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse;
    fn display(&self, input_state: InputState, list: &mut DisplayList);
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
            keyboard_focus: None,
            mouse_focus: None,
            mouse_position_captured: false,
        }
    }

    pub fn get_min_size(&self) -> (f32, f32) {
        self.root.borrow().get_min_size()
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

    pub fn handle_event(&mut self, ev: InputEvent) -> UIEventResponse {
        match ev {
            InputEvent::CursorMoved { position } => {
                if self.mouse_position_captured {
                    if let Some(mouse_drag_origin) = self.input_state.mouse_drag_origin {
                        self.input_state.mouse_position += position - mouse_drag_origin;
                    } else {
                        self.input_state.mouse_position = position;
                    }
                } else {
                    self.input_state.mouse_position = position;
                }
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

        let mut ui_response: UIEventResponse = Default::default();

        let response = match ev {
            InputEvent::CursorMoved { .. } | InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. } | InputEvent::MouseScroll { .. } => {
                if let Some(ref focus) = self.mouse_focus {
                    focus.borrow_mut().handle_event(ev, self.input_state)
                } else {
                    self.root.borrow_mut().handle_event(ev, self.input_state)
                }
            },
            InputEvent::KeyPress { .. } | InputEvent::KeyRelease { .. } | InputEvent::TextInput { .. } => {
                if let Some(ref focus) = self.keyboard_focus {
                    focus.borrow_mut().handle_event(ev, self.input_state)
                } else {
                    self.root.borrow_mut().handle_event(ev, self.input_state)
                }
            }
            _ => {
                self.root.borrow_mut().handle_event(ev, self.input_state)
            }
        };

        if response.capture_keyboard {
            if let Some(ref focus) = self.keyboard_focus {
                focus.borrow_mut().handle_event(InputEvent::LostKeyboardFocus, self.input_state);
            }
            if let Some(ref responder) = response.responder {
                self.keyboard_focus = Some(responder.clone());
            }
        }
        if response.capture_mouse {
            if let Some(ref responder) = response.responder {
                self.mouse_focus = Some(responder.clone());
            }
        }
        if response.capture_mouse_position {
            self.mouse_position_captured = true;
        }
        if self.mouse_position_captured {
            if let Some(mouse_drag_origin) = self.input_state.mouse_drag_origin {
                ui_response.set_mouse_position = Some((mouse_drag_origin.x, mouse_drag_origin.y));
            }
        }
        ui_response.mouse_cursor = response.mouse_cursor;

        match ev {
            InputEvent::MouseRelease { button: MouseButton::Left } => {
                if self.mouse_position_captured {
                    if let Some(mouse_drag_origin) = self.input_state.mouse_drag_origin {
                        self.input_state.mouse_position = mouse_drag_origin;
                    }
                    self.mouse_position_captured = false;
                }

                self.input_state.mouse_drag_origin = None;
                self.mouse_focus = None;
            }
            _ => {}
        }

        ui_response
    }

    pub fn display(&self) -> DisplayList {
        let mut list = DisplayList::new();
        self.root.borrow().display(self.input_state, &mut list);
        list
    }
}


pub struct Empty;

impl Empty {
    pub fn new() -> Rc<RefCell<Empty>> {
        Rc::new(RefCell::new(Empty))
    }
}

impl Widget for Empty {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {}
    fn get_min_size(&self) -> (f32, f32) { (0.0, 0.0) }
    fn get_size(&self) -> (f32, f32) { (0.0, 0.0) }
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse { Default::default() }
    fn display(&self, input_state: InputState, list: &mut DisplayList) { }
}


pub struct Container {
    child: WidgetRef,
    style: ContainerStyle,
    container_width: Option<f32>,
    container_height: Option<f32>,
}

impl Container {
    pub fn new(child: WidgetRef) -> Rc<RefCell<Container>> {
        Rc::new(RefCell::new(Container { child: child, style: Default::default(), container_width: None, container_height: None }))
    }

    pub fn get_style(&mut self) -> &mut ContainerStyle {
        &mut self.style
    }

    fn get_max_size(&self) -> (Option<f32>, Option<f32>) {
        let max_width = self.container_width
            .and_then(|container_width| self.style.max_width.and_then(|max_width| Some(container_width.min(max_width))))
            .or(self.container_width).or(self.style.max_width);
        let max_height = self.container_height
            .and_then(|container_height| self.style.max_height.and_then(|max_height| Some(container_height.min(max_height))))
            .or(self.container_height).or(self.style.max_height);
        (max_width, max_height)
    }

    fn get_size_from_child_size(&self, child_width: f32, child_height: f32) -> (f32, f32) {
        let (max_width, max_height) = self.get_max_size();
        let width = if max_width.is_some() && self.style.h_fill {
            max_width.unwrap()
        } else {
            let contents_width = child_width + 2.0 * self.style.padding;
            self.style.min_width.map_or(contents_width, |min_width| min_width.max(contents_width))
        };
        let height = if max_height.is_some() && self.style.v_fill {
            max_height.unwrap()
        } else {
            let contents_height = child_height + 2.0 * self.style.padding;
            self.style.min_height.map_or(contents_height, |min_height| min_height.max(contents_height))
        };
        (width, height)
    }

    fn get_layout(&self) -> ((f32, f32), (f32, f32)) {
        let (child_width, child_height) = self.child.borrow().get_size();

        let (width, height) = self.get_size_from_child_size(child_width, child_height);
        let x_offset = match self.style.h_align {
            Align::Beginning => self.style.padding,
            Align::Center => width / 2.0 - child_width / 2.0,
            Align::End => width - self.style.padding - child_width,
        };
        let y_offset = match self.style.v_align {
            Align::Beginning => self.style.padding,
            Align::Center => height / 2.0 - child_height / 2.0,
            Align::End => height - self.style.padding - child_height,
        };

        ((x_offset, y_offset), (width, height))
    }
}

impl Widget for Container {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.container_width = width;
        self.container_height = height;
        let (max_width, max_height) = self.get_max_size();
        self.child.borrow_mut().set_container_size(max_width.map(|max_width| max_width - 2.0 * self.style.padding), max_height.map(|max_height| max_height - 2.0 * self.style.padding));
    }

    fn get_min_size(&self) -> (f32, f32) {
        let (child_width, child_height) = self.child.borrow().get_min_size();
        let contents_width = child_width + 2.0 * self.style.padding;
        let contents_height = child_height + 2.0 * self.style.padding;
        let min_width = self.style.min_width.map_or(contents_width, |min_width| min_width.max(contents_width));
        let min_height = self.style.min_height.map_or(contents_height, |min_height| min_height.max(contents_height));
        (min_width, min_height)
    }

    fn get_size(&self) -> (f32, f32) {
        let (child_width, child_height) = self.child.borrow().get_size();
        self.get_size_from_child_size(child_width, child_height)
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
        let ((x, y), (width, height)) = self.get_layout();
        let mut response = self.child.borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: -y }));
        response.responder = response.responder.or_else(|| Some(self.child.clone()));
        response
    }

    fn display(&self, input_state: InputState, list: &mut DisplayList) {
        let ((x, y), (width, height)) = self.get_layout();
        list.rect(Rect { x: 0.0, y: 0.0, w: width, h: height, color: self.style.background_color });
        list.push_translate(Point { x: x, y: y });
        self.child.borrow().display(input_state.translate(Point { x: -self.style.padding, y: -self.style.padding }), list);
        list.pop_translate();
    }
}

pub struct ContainerStyle {
    pub padding: f32,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    pub h_align: Align,
    pub v_align: Align,
    pub h_fill: bool,
    pub v_fill: bool,
    pub background_color: [f32; 4],
}

pub enum Align { Beginning, Center, End }

impl Default for ContainerStyle {
    fn default() -> ContainerStyle {
        ContainerStyle {
            padding: 8.0,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            h_align: Align::Beginning,
            v_align: Align::Beginning,
            h_fill: false,
            v_fill: false,
            background_color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}


pub struct Flex {
    children: Vec<WidgetRef>,
    style: FlexStyle,
    container_width: Option<f32>,
    container_height: Option<f32>,
}

impl Flex {
    pub fn new(children: Vec<WidgetRef>, axis: FlexAxis) -> Rc<RefCell<Flex>> {
        Rc::new(RefCell::new(Flex { children: children, style: FlexStyle { axis: axis, ..Default::default() }, container_width: None, container_height: None }))
    }

    pub fn row(children: Vec<WidgetRef>) -> Rc<RefCell<Flex>> {
        Flex::new(children, FlexAxis::Horizontal)
    }

    pub fn col(children: Vec<WidgetRef>) -> Rc<RefCell<Flex>> {
        Flex::new(children, FlexAxis::Vertical)
    }

    pub fn get_child(&self, i: usize) -> WidgetRef {
        self.children[i].clone()
    }

    pub fn get_style(&mut self) -> &mut FlexStyle {
        &mut self.style
    }

    fn get_max_size(&self) -> (Option<f32>, Option<f32>) {
        let max_width = self.container_width
            .and_then(|container_width| self.style.max_width.and_then(|max_width| Some(container_width.min(max_width))))
            .or(self.container_width).or(self.style.max_width);
        let max_height = self.container_height
            .and_then(|container_height| self.style.max_height.and_then(|max_height| Some(container_height.min(max_height))))
            .or(self.container_height).or(self.style.max_height);
        (max_width, max_height)
    }

    fn get_length_from_child_lengths(&self, child_lengths: &Vec<f32>, max_length: Option<f32>) -> Vec<f32> {
        let mut container_lengths: Vec<f32> = child_lengths.clone();
        let children_length = container_lengths.iter().sum::<f32>() + 2.0 * self.style.padding + self.style.spacing * (self.children.len() - 1) as f32;
        let length = self.get_length_from_children_length(children_length, max_length);
        let extra_space = length - children_length - 2.0 * self.style.padding - self.style.spacing * (self.children.len() - 1) as f32;
        match self.style.main_fill {
            Grow::None => {}
            Grow::Equal => {
                let count = self.children.len() as f32;
                for child_length in container_lengths.iter_mut() {
                    *child_length += extra_space / count;
                }
            }
            Grow::Ratio(ref amounts) => {
                let total: f32 = amounts.iter().sum();
                for (i, child_length) in container_lengths.iter_mut().enumerate() {
                    *child_length += (amounts[i] / total) * extra_space;
                }
            }
        }
        container_lengths
    }

    fn get_length_from_children_length(&self, children_length: f32, max_length: Option<f32>) -> f32 {
        let length = if max_length.is_some() && self.style.main_fill != Grow::None {
            max_length.unwrap()
        } else {
            let contents_length = children_length + 2.0 * self.style.padding + self.style.spacing * (self.children.len() - 1) as f32;
            self.main_axis((self.style.min_width, self.style.min_height)).map_or(contents_length, |min_length| min_length.max(contents_length))
        };
        length
    }

    fn get_cross_length_from_child_cross_lengths(&self, child_cross_lengths: &Vec<f32>, max_cross_length: Option<f32>) -> f32 {
        let cross_length = if max_cross_length.is_some() && self.style.cross_fill {
            max_cross_length.unwrap()
        } else {
            let mut contents_cross_length: f32 = 0.0;
            for child_cross_length in child_cross_lengths {
                contents_cross_length = contents_cross_length.max(*child_cross_length);
            }
            self.cross_axis((self.style.min_width, self.style.min_height)).map_or(contents_cross_length, |min_cross_length| min_cross_length.max(contents_cross_length))
        };
        cross_length
    }

    fn get_layout(&self) -> (Vec<(f32, f32)>, Vec<(f32, f32)>, (f32, f32)) {
        let max_size = self.get_max_size();
        let child_sizes: Vec<(f32, f32)> = self.children.iter().map(|child| child.borrow().get_size()).collect();
        let container_lengths = self.get_length_from_child_lengths(&child_sizes.iter().map(|child_size| self.main_axis(*child_size)).collect(), self.main_axis(max_size));
        let children_length = container_lengths.iter().sum::<f32>() + 2.0 * self.style.padding + self.style.spacing * (self.children.len() - 1) as f32;
        let length = self.get_length_from_children_length(children_length, self.main_axis(max_size));
        let cross_length = self.get_cross_length_from_child_cross_lengths(&child_sizes.iter().map(|child_size| self.cross_axis(*child_size)).collect(), self.cross_axis(max_size));

        let mut child_offsets = Vec::with_capacity(self.children.len());
        let mut offset = self.style.padding;
        for i in 0..self.children.len() {
            let child_size = child_sizes[i];
            let container_length = container_lengths[i];

            let main = offset + match self.style.main_align {
                Align::Beginning => 0.0,
                Align::Center => container_length / 2.0 - self.main_axis(child_size) / 2.0,
                Align::End => container_length - self.main_axis(child_size),
            };
            let cross = match self.style.cross_align {
                Align::Beginning => self.style.padding,
                Align::Center => cross_length / 2.0 - self.cross_axis(child_size) / 2.0,
                Align::End => cross_length - self.style.padding - self.cross_axis(child_size),
            };
            child_offsets.push(self.main_cross_to_x_y((main, cross)));

            offset += container_length + self.style.spacing;
        }

        (child_offsets, child_sizes, self.main_cross_to_x_y((length, cross_length)))
    }

    fn main_axis<T>(&self, size: (T, T)) -> T {
        match self.style.axis {
            FlexAxis::Horizontal => size.0,
            FlexAxis::Vertical => size.1,
        }
    }

    fn cross_axis<T>(&self, size: (T, T)) -> T {
        match self.style.axis {
            FlexAxis::Horizontal => size.1,
            FlexAxis::Vertical => size.0,
        }
    }

    fn main_cross_to_x_y<T>(&self, main_cross: (T, T)) -> (T, T) {
        match self.style.axis {
            FlexAxis::Horizontal => (main_cross.0, main_cross.1),
            FlexAxis::Vertical => (main_cross.1, main_cross.0),
        }
    }
}

impl Widget for Flex {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.container_width = width;
        self.container_height = height;
        let max_size = self.get_max_size();
        let min_child_lengths: Vec<f32> = self.children.iter().map(|child| self.main_axis(child.borrow().get_min_size())).collect();
        let container_lengths = self.get_length_from_child_lengths(&min_child_lengths, self.main_axis(max_size));
        for (i, child) in self.children.iter().enumerate() {
            let (container_width, container_height) = self.main_cross_to_x_y((Some(container_lengths[i]), self.cross_axis(max_size)));
            child.borrow_mut().set_container_size(container_width, container_height);
        }
    }

    fn get_min_size(&self) -> (f32, f32) {
        let mut main: f32 = 0.0;
        let mut cross: f32 = 0.0;
        for child in self.children.iter() {
            let child_min_size = child.borrow().get_min_size();
            main += self.main_axis(child_min_size) + self.style.spacing;
            cross = cross.max(self.cross_axis(child_min_size));
        }
        let (contents_width, contents_height) = self.main_cross_to_x_y((main + 2.0 * self.style.padding, cross + 2.0 * self.style.padding));
        let min_width = self.style.min_width.map_or(contents_width, |min_width| min_width.max(contents_width));
        let min_height = self.style.min_height.map_or(contents_height, |min_height| min_height.max(contents_height));
        (min_width, min_height)
    }

    fn get_size(&self) -> (f32, f32) {
        let (_, _, (width, height)) = self.get_layout();
        (width, height)
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
        let (child_offsets, child_sizes, _) = self.get_layout();

        match ev {
            InputEvent::CursorMoved { .. } | InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. } | InputEvent::MouseScroll { .. } => {
                let mouse_position = input_state.mouse_drag_origin.unwrap_or(input_state.mouse_position);

                for (i, child) in self.children.iter().enumerate() {
                    let (x, y) = child_offsets[i];
                    if x <= mouse_position.x && mouse_position.x < x + child_sizes[i].0 && y <= mouse_position.y && mouse_position.y < y + child_sizes[i].1 {
                        let mut response = child.borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: -y }));
                        response.responder = response.responder.or_else(|| Some(child.clone()));
                        return response;
                    }
                }
            }
            _ => {}
        }

        Default::default()
    }

    fn display(&self, input_state: InputState, list: &mut DisplayList) {
        let (child_offsets, child_sizes, (width, height)) = self.get_layout();

        list.rect(Rect { x: 0.0, y: 0.0, w: width, h: height, color: self.style.background_color });
        for (i, child) in self.children.iter().enumerate() {
            let (x, y) = child_offsets[i];
            list.push_translate(Point { x: x, y: y });
            child.borrow().display(input_state.translate(Point { x: -x, y: -y }), list);
            list.pop_translate();
        }
    }
}

pub struct FlexStyle {
    pub axis: FlexAxis,
    pub padding: f32,
    pub spacing: f32,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    pub main_align: Align,
    pub cross_align: Align,
    pub main_fill: Grow,
    pub cross_fill: bool,
    pub background_color: [f32; 4],
}

#[derive(Clone, PartialEq)]
pub enum Grow {
    None,
    Equal,
    Ratio(Vec<f32>),
}

#[derive(Clone, PartialEq)]
pub enum FlexAxis {
    Horizontal,
    Vertical,
}

impl Default for FlexStyle {
    fn default() -> FlexStyle {
        FlexStyle {
            axis: FlexAxis::Horizontal,
            padding: 0.0,
            spacing: 0.0,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            main_align: Align::Beginning,
            cross_align: Align::Beginning,
            main_fill: Grow::None,
            cross_fill: false,
            background_color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}


pub struct Button {
    contents: WidgetRef,
    on_press: Option<Box<Fn()>>,
}

impl Button {
    pub fn new(contents: WidgetRef) -> Rc<RefCell<Button>> {
        Rc::new(RefCell::new(Button { contents: Container::new(contents), on_press: None }))
    }

    pub fn with_text(text: &'static str, font: Rc<Font<'static>>) -> Rc<RefCell<Button>> {
        Button::new(Label::new(text, font))
    }

    pub fn on_press<F>(&mut self, callback: F) where F: 'static + Fn() {
        self.on_press = Some(Box::new(callback));
    }
}

impl Widget for Button {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_min_size(&self) -> (f32, f32) {
        self.contents.borrow().get_min_size()
    }

    fn get_size(&self) -> (f32, f32) {
        self.contents.borrow().get_size()
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
        match ev {
            InputEvent::MouseRelease { button: MouseButton::Left } => {
                let (width, height) = self.get_size();
                if 0.0 < input_state.mouse_position.x && input_state.mouse_position.x < width && 0.0 < input_state.mouse_position.y && input_state.mouse_position.y < height {
                    if let Some(ref on_press) = self.on_press {
                        on_press();
                    }
                }
            }
            _ => {}
        }

        Default::default()
    }

    fn display(&self, input_state: InputState, list: &mut DisplayList) {
        let (width, height) = self.get_size();

        let mut color = [0.15, 0.18, 0.23, 1.0];
        if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
            if 0.0 <= mouse_drag_origin.x && mouse_drag_origin.x < width && 0.0 <= mouse_drag_origin.y && mouse_drag_origin.y < height {
                color = [0.02, 0.2, 0.6, 1.0];
            }
        } else if 0.0 <= input_state.mouse_position.x && input_state.mouse_position.x < width && 0.0 <= input_state.mouse_position.y && input_state.mouse_position.y < height {
            color = [0.3, 0.4, 0.5, 1.0];
        }

        list.rect(Rect { x: 0.0, y: 0.0, w: width, h: height, color: color });
        self.contents.borrow().display(input_state, list);
    }
}


pub struct Label {
    text: String,
    font: Rc<Font<'static>>,
    scale: Scale,
    glyphs: Vec<PositionedGlyph<'static>>,
    height: f32,
    width: f32,
}

impl Label {
    pub fn new(text: &str, font: Rc<Font<'static>>) -> Rc<RefCell<Label>> {
        let mut label = Label {
            text: String::new(),
            font: font,
            scale: Scale::uniform(14.0),
            glyphs: Vec::with_capacity(text.len()),
            width: 0.0,
            height: 0.0,
        };
        label.set_text(text);
        Rc::new(RefCell::new(label))
    }

    pub fn set_text(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
        self.layout_text();
    }

    pub fn modify_text<F>(&mut self, closure: F) where F: 'static + Fn(&mut String) {
        closure(&mut self.text);
        self.layout_text();
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    fn layout_text(&mut self) {
        use unicode_normalization::UnicodeNormalization;
        self.glyphs.clear();

        let v_metrics = self.font.v_metrics(self.scale);
        let mut caret = point(0.0, v_metrics.ascent);
        let mut last_glyph_id = None;
        for c in self.text.nfc() {
            if c.is_control() {
                continue;
            }
            let base_glyph = if let Some(glyph) = self.font.glyph(c) {
                glyph
            } else {
                continue;
            };
            if let Some(id) = last_glyph_id.take() {
                let glyph_width = self.font.pair_kerning(self.scale, id, base_glyph.id());
                caret.x += glyph_width;
            }
            last_glyph_id = Some(base_glyph.id());
            let glyph = base_glyph.scaled(self.scale).positioned(caret);
            caret.x += glyph.unpositioned().h_metrics().advance_width;

            self.glyphs.push(glyph.standalone());
        }

        self.height = v_metrics.ascent - v_metrics.descent;
        self.width = caret.x;
    }

}

impl Widget for Label {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_min_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn get_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
        Default::default()
    }

    fn display(&self, input_state: InputState, list: &mut DisplayList) {
        for glyph in self.glyphs.iter() {
            list.glyph(glyph.standalone());
        }
    }
}


pub struct Textbox {
    label: Rc<RefCell<Label>>,
    on_change: Option<Box<Fn(&str)>>,
}

impl Textbox {
    pub fn new(font: Rc<Font<'static>>) -> Rc<RefCell<Textbox>> {
        Rc::new(RefCell::new(Textbox { label: Label::new("", font), on_change: None }))
    }

    pub fn on_change<F>(&mut self, callback: F) where F: 'static + Fn(&str) {
        self.on_change = Some(Box::new(callback));
    }
}

impl Widget for Textbox {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.label.borrow_mut().set_container_size(width, height);
    }

    fn get_min_size(&self) -> (f32, f32) {
        self.label.borrow().get_size()
    }

    fn get_size(&self) -> (f32, f32) {
        let (width, height) = self.label.borrow().get_size();
        (width.max(40.0), height)
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
        match ev {
            InputEvent::KeyPress { button: KeyboardButton::Back } => {
                let mut label = self.label.borrow_mut();
                label.modify_text(|text| { text.pop(); () });
                if let Some(ref on_change) = self.on_change {
                    on_change(label.get_text());
                }
            }
            InputEvent::TextInput { character: c } => {
                if !c.is_control() {
                    let mut label = self.label.borrow_mut();
                    label.modify_text(move |text| text.push(c));
                    if let Some(ref on_change) = self.on_change {
                        on_change(label.get_text());
                    }
                }
            }
            _ => {}
        }

        Default::default()
    }

    fn display(&self, input_state: InputState, list: &mut DisplayList) {
        let color = [0.1, 0.15, 0.2, 1.0];
        let (width, height) = self.get_size();
        list.rect(Rect { x: 0.0, y: 0.0, w: width.max(40.0), h: height, color: color });

        self.label.borrow().display(input_state, list);
    }
}


pub struct IntegerInput {
    value: i32,
    new_value: Option<i32>,
    on_change: Option<Box<Fn(i32)>>,
    format: Option<Box<Fn(i32) -> String>>,
    container: Rc<RefCell<Container>>,
    label: Rc<RefCell<Label>>,
}

impl IntegerInput {
    pub fn new(value: i32, font: Rc<Font<'static>>) -> Rc<RefCell<IntegerInput>> {
        let label = Label::new(&value.to_string(), font);
        let container = Container::new(label.clone());
        container.borrow_mut().get_style().padding = 2.0;
        Rc::new(RefCell::new(IntegerInput { value: value, new_value: None, on_change: None, format: None, container: container, label: label }))
    }

    pub fn set_value(&mut self, value: i32) {
        self.value = value;
        self.render_text(value);
    }

    pub fn on_change<F>(&mut self, callback: F) where F: 'static + Fn(i32) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn format<F>(&mut self, callback: F) where F: 'static + Fn(i32) -> String {
        self.label.borrow_mut().set_text(&callback(self.value));
        self.format = Some(Box::new(callback));
    }

    fn render_text(&mut self, value: i32) {
        let text = if let Some(ref format) = self.format {
            format(value)
        } else {
            value.to_string()
        };
        self.label.borrow_mut().set_text(&text);
    }
}

impl Widget for IntegerInput {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.container.borrow_mut().set_container_size(width, height);
    }

    fn get_min_size(&self) -> (f32, f32) {
        self.container.borrow().get_min_size()
    }

    fn get_size(&self) -> (f32, f32) {
        self.container.borrow().get_size()
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
        match ev {
            InputEvent::CursorMoved { position } => {
                if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
                    let dy = -(input_state.mouse_position.y - mouse_drag_origin.y);
                    let new_value = self.value + (dy / 8.0) as i32;
                    self.new_value = Some(new_value);
                    self.render_text(new_value);
                    if let Some(ref on_change) = self.on_change {
                        on_change(new_value);
                    }
                    return EventResponse {
                        capture_mouse: true,
                        capture_mouse_position: true,
                        mouse_cursor: MouseCursor::NoneCursor,
                        ..Default::default()
                    };
                }
            }
            InputEvent::MouseRelease { button: MouseButton::Left } => {
                if let Some(new_value) = self.new_value {
                    self.value = new_value;
                    self.new_value = None;
                }
            }
            _ => {}
        }

        Default::default()
    }

    fn display(&self, input_state: InputState, list: &mut DisplayList) {
        self.container.borrow().display(input_state, list);
    }
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MouseCursor {
    Default,
    Crosshair,
    Hand,
    Arrow,
    Move,
    Text,
    Wait,
    Help,
    Progress,
    NotAllowed,
    ContextMenu,
    NoneCursor,
    Cell,
    VerticalText,
    Alias,
    Copy,
    NoDrop,
    Grab,
    Grabbing,
    AllScroll,
    ZoomIn,
    ZoomOut,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
}

impl MouseCursor {
    pub fn to_glutin(cursor: MouseCursor) -> glutin::MouseCursor {
        match cursor {
            MouseCursor::Default => glutin::MouseCursor::Default,
            MouseCursor::Crosshair => glutin::MouseCursor::Crosshair,
            MouseCursor::Hand => glutin::MouseCursor::Hand,
            MouseCursor::Arrow => glutin::MouseCursor::Arrow,
            MouseCursor::Move => glutin::MouseCursor::Move,
            MouseCursor::Text => glutin::MouseCursor::Text,
            MouseCursor::Wait => glutin::MouseCursor::Wait,
            MouseCursor::Help => glutin::MouseCursor::Help,
            MouseCursor::Progress => glutin::MouseCursor::Progress,
            MouseCursor::NotAllowed => glutin::MouseCursor::NotAllowed,
            MouseCursor::ContextMenu => glutin::MouseCursor::ContextMenu,
            MouseCursor::NoneCursor => glutin::MouseCursor::NoneCursor,
            MouseCursor::Cell => glutin::MouseCursor::Cell,
            MouseCursor::VerticalText => glutin::MouseCursor::VerticalText,
            MouseCursor::Alias => glutin::MouseCursor::Alias,
            MouseCursor::Copy => glutin::MouseCursor::Copy,
            MouseCursor::NoDrop => glutin::MouseCursor::NoDrop,
            MouseCursor::Grab => glutin::MouseCursor::Grab,
            MouseCursor::Grabbing => glutin::MouseCursor::Grabbing,
            MouseCursor::AllScroll => glutin::MouseCursor::AllScroll,
            MouseCursor::ZoomIn => glutin::MouseCursor::ZoomIn,
            MouseCursor::ZoomOut => glutin::MouseCursor::ZoomOut,
            MouseCursor::EResize => glutin::MouseCursor::EResize,
            MouseCursor::NResize => glutin::MouseCursor::NResize,
            MouseCursor::NeResize => glutin::MouseCursor::NeResize,
            MouseCursor::NwResize => glutin::MouseCursor::NwResize,
            MouseCursor::SResize => glutin::MouseCursor::SResize,
            MouseCursor::SeResize => glutin::MouseCursor::SeResize,
            MouseCursor::SwResize => glutin::MouseCursor::SwResize,
            MouseCursor::WResize => glutin::MouseCursor::WResize,
            MouseCursor::EwResize => glutin::MouseCursor::EwResize,
            MouseCursor::NsResize => glutin::MouseCursor::NsResize,
            MouseCursor::NeswResize => glutin::MouseCursor::NeswResize,
            MouseCursor::NwseResize => glutin::MouseCursor::NwseResize,
            MouseCursor::ColResize => glutin::MouseCursor::ColResize,
            MouseCursor::RowResize => glutin::MouseCursor::RowResize,
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

impl ops::AddAssign for Point {
    fn add_assign(&mut self, other: Point) {
        *self = *self + other;
    }
}

impl ops::Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}

impl ops::SubAssign for Point {
    fn sub_assign(&mut self, other: Point) {
        *self = *self - other;
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

impl ops::MulAssign<f32> for Point {
    fn mul_assign(&mut self, other: f32) {
        *self = *self * other;
    }
}
