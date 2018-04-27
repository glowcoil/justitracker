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
    fn set_container_size(&mut self, w: Option<f32>, h: Option<f32>);
    fn get_min_size(&self) -> (f32, f32);
    fn get_size(&self) -> (f32, f32);
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState);
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

        match ev {
            InputEvent::MouseRelease { button: MouseButton::Left } => {
                self.input_state.mouse_drag_origin = None;
            }
            _ => {}
        }
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
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {}
    fn get_min_size(&self) -> (f32, f32) { (0.0, 0.0) }
    fn get_size(&self) -> (f32, f32) { (0.0, 0.0) }
    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {}
    fn display(&self, input_state: InputState) -> DisplayList { DisplayList::new() }
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

    fn get_child_offset(&self, child_width: f32, child_height: f32) -> (f32, f32) {
        let (width, height) = self.get_size_from_child_size(child_width, child_height);
        let x = match self.style.h_align {
            HAlign::Left => self.style.padding,
            HAlign::Center => width / 2.0 - child_width / 2.0,
            HAlign::Right => width - self.style.padding - child_width,
        };
        let y = match self.style.v_align {
            VAlign::Top => self.style.padding,
            VAlign::Center => height / 2.0 - child_height / 2.0,
            VAlign::Bottom => height - self.style.padding - child_height,
        };
        (x, y)
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

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        let (child_width, child_height) = self.child.borrow().get_size();
        let (x, y) = self.get_child_offset(child_width, child_height);
        self.child.borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: -y }));
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let (child_width, child_height) = self.child.borrow().get_size();
        let (x, y) = self.get_child_offset(child_width, child_height);
        let mut list = self.child.borrow().display(input_state.translate(Point { x: -self.style.padding, y: -self.style.padding }));
        list.translate(Point { x: x, y: y });
        list
    }
}

pub struct ContainerStyle {
    padding: f32,
    min_width: Option<f32>,
    max_width: Option<f32>,
    min_height: Option<f32>,
    max_height: Option<f32>,
    h_align: HAlign,
    v_align: VAlign,
    h_fill: bool,
    v_fill: bool,
}

pub enum HAlign { Left, Center, Right }
pub enum VAlign { Top, Center, Bottom }

impl Default for ContainerStyle {
    fn default() -> ContainerStyle {
        ContainerStyle {
            padding: 8.0,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            h_align: HAlign::Left,
            v_align: VAlign::Top,
            h_fill: false,
            v_fill: false,
        }
    }
}


pub struct Row {
    children: Vec<WidgetRef>,
    focus: Option<usize>,
    style: RowStyle,
    container_width: Option<f32>,
    container_height: Option<f32>,
}

impl Row {
    pub fn new(children: Vec<WidgetRef>) -> Rc<RefCell<Row>> {
        Rc::new(RefCell::new(Row { children: children, focus: None, style: Default::default(), container_width: None, container_height: None }))
    }

    pub fn get_child(&self, i: usize) -> WidgetRef {
        self.children[i].clone()
    }

    pub fn get_style(&mut self) -> &mut RowStyle {
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

    fn get_widths_from_child_widths(&self, child_widths: &Vec<f32>, max_width: Option<f32>) -> Vec<f32> {
        let mut container_widths: Vec<f32> = child_widths.clone();
        let children_width: f32 = container_widths.iter().sum();
        if let Some(max_width) = max_width {
            let extra_space = max_width - children_width - 2.0 * self.style.padding - self.style.spacing * (self.children.len() - 1) as f32;
            match self.style.h_fill {
                Grow::None => {}
                Grow::Equal => {
                    let count = self.children.len() as f32;
                    for child_width in container_widths.iter_mut() {
                        *child_width += extra_space / count;
                    }
                }
                Grow::Ratio(ref amounts) => {
                    let total: f32 = amounts.iter().sum();
                    for (i, child_width) in container_widths.iter_mut().enumerate() {
                        *child_width += (amounts[i] / total) * extra_space;
                    }
                }
            }
        }
        container_widths
    }

    fn get_height_from_child_heights(&self, child_heights: &Vec<f32>, max_height: Option<f32>) -> f32 {
        let height = if max_height.is_some() && self.style.v_fill {
            max_height.unwrap()
        } else {
            let mut contents_height: f32 = 0.0;
            for child_height in child_heights {
                contents_height = contents_height.max(*child_height);
            }
            self.style.min_height.map_or(contents_height, |min_height| min_height.max(contents_height))
        };
        height
    }

    fn get_child_sizes(&self) -> (Vec<f32>, Vec<f32>) {
        let mut child_widths = Vec::with_capacity(self.children.len());
        let mut child_heights = Vec::with_capacity(self.children.len());
        for child in self.children.iter() {
            let (child_width, child_height) = child.borrow().get_size();
            child_widths.push(child_width);
            child_heights.push(child_height)
        }
        (child_widths, child_heights)
    }

    fn get_layout(&self) -> (Vec<(f32, f32)>, Vec<f32>, Vec<f32>) {
        let (max_width, max_height) = self.get_max_size();
        let (child_widths, child_heights) = self.get_child_sizes();
        let container_widths = self.get_widths_from_child_widths(&child_widths, max_width);
        let height = self.get_height_from_child_heights(&child_heights, max_height);

        let mut child_offsets = Vec::with_capacity(self.children.len());
        let mut x_offset = self.style.padding;
        for i in 0..self.children.len() {
            let child_width = child_widths[i];
            let child_height = child_heights[i];
            let container_width = container_widths[i];

            let x = x_offset + match self.style.h_align {
                HAlign::Left => 0.0,
                HAlign::Center => container_width / 2.0 - child_width / 2.0,
                HAlign::Right => container_width - child_width,
            };
            let y = match self.style.v_align {
                VAlign::Top => self.style.padding,
                VAlign::Center => height / 2.0 - child_height / 2.0,
                VAlign::Bottom => height - self.style.padding - child_height,
            };
            child_offsets.push((x, y));

            x_offset += container_width + self.style.spacing;
        }

        (child_offsets, child_widths, child_heights)
    }
}

impl Widget for Row {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.container_width = width;
        self.container_height = height;
        let (max_width, max_height) = self.get_max_size();
        let min_child_widths: Vec<f32> = self.children.iter().map(|child| child.borrow().get_min_size().0).collect();
        let container_widths = self.get_widths_from_child_widths(&min_child_widths, max_width);
        for (i, child) in self.children.iter().enumerate() {
            child.borrow_mut().set_container_size(Some(container_widths[i]), max_height);
        }
    }

    fn get_min_size(&self) -> (f32, f32) {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in self.children.iter() {
            let (child_width, child_height) = child.borrow().get_min_size();
            width += child_width + self.style.spacing;
            height = height.max(child_height);
        }
        (width + 2.0 * self.style.padding, height + 2.0 * self.style.padding)
    }

    fn get_size(&self) -> (f32, f32) {
        let (max_width, max_height) = self.get_max_size();
        let (child_widths, child_heights) = self.get_child_sizes();
        let container_widths = self.get_widths_from_child_widths(&child_widths, max_width);

        let width = container_widths.iter().sum::<f32>() + 2.0 * self.style.padding + self.style.spacing * (self.children.len() - 1) as f32;
        let height = self.get_height_from_child_heights(&child_heights, max_height);
        (width, height)
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        let (child_offsets, child_widths, child_heights) = self.get_layout();

        match ev {
            InputEvent::CursorMoved { .. } | InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. } | InputEvent::MouseScroll { .. } => {
                let mouse_position = input_state.mouse_drag_origin.unwrap_or(input_state.mouse_position);

                for (i, child) in self.children.iter().enumerate() {
                    let (x, y) = child_offsets[i];
                    if x <= mouse_position.x && mouse_position.x < x + child_widths[i] && y <= mouse_position.y && mouse_position.y < y + child_heights[i] {
                        if let InputEvent::MousePress { .. } = ev {
                            self.focus = Some(i);
                        }
                        child.borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: -y }));
                        break;
                    }
                }
            },
            InputEvent::KeyPress { .. } | InputEvent::KeyRelease { .. } | InputEvent::TextInput { .. } => {
                if let Some(focus) = self.focus {
                    let (x, y) = child_offsets[focus];
                    self.children[focus].borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: -y }));
                }
            },
        }
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let (child_offsets, _child_widths, _child_heights) = self.get_layout();

        let mut list = DisplayList::new();
        for (i, child) in self.children.iter().enumerate() {
            let (x, y) = child_offsets[i];
            let mut child_list = child.borrow().display(input_state.translate(Point { x: -x, y: -y }));
            child_list.translate(Point { x: x, y: y });
            list.merge(child_list);
        }

        list
    }
}

pub struct RowStyle {
    padding: f32,
    spacing: f32,
    min_width: Option<f32>,
    max_width: Option<f32>,
    min_height: Option<f32>,
    max_height: Option<f32>,
    h_align: HAlign,
    v_align: VAlign,
    h_fill: Grow,
    v_fill: bool,
}

#[derive(Clone)]
pub enum Grow {
    None,
    Equal,
    Ratio(Vec<f32>),
}

impl Default for RowStyle {
    fn default() -> RowStyle {
        RowStyle {
            padding: 0.0,
            spacing: 0.0,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            h_align: HAlign::Left,
            v_align: VAlign::Top,
            h_fill: Grow::None,
            v_fill: false,
        }
    }
}


pub struct Column {
    children: Vec<WidgetRef>,
    focus: Option<usize>,
    style: ColumnStyle,
    container_width: Option<f32>,
    container_height: Option<f32>,
}

impl Column {
    pub fn new(children: Vec<WidgetRef>) -> Rc<RefCell<Column>> {
        Rc::new(RefCell::new(Column { children: children, focus: None, style: Default::default(), container_width: None, container_height: None }))
    }

    pub fn get_child(&self, i: usize) -> WidgetRef {
        self.children[i].clone()
    }

    pub fn get_style(&mut self) -> &mut ColumnStyle {
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

    fn get_width_from_child_widths(&self, child_widths: &Vec<f32>, max_width: Option<f32>) -> f32 {
        let width = if max_width.is_some() && self.style.h_fill {
            max_width.unwrap()
        } else {
            let mut contents_width: f32 = 0.0;
            for child_width in child_widths {
                contents_width = contents_width.max(*child_width);
            }
            self.style.min_width.map_or(contents_width, |min_width| min_width.max(contents_width))
        };
        width
    }

    fn get_heights_from_child_heights(&self, child_heights: &Vec<f32>, max_height: Option<f32>) -> Vec<f32> {
        let mut container_heights: Vec<f32> = child_heights.clone();
        let children_height: f32 = container_heights.iter().sum();
        if let Some(max_height) = max_height {
            let extra_space = max_height - children_height - 2.0 * self.style.padding - self.style.spacing * (self.children.len() - 1) as f32;
            match self.style.v_fill {
                Grow::None => {}
                Grow::Equal => {
                    let count = self.children.len() as f32;
                    for child_height in container_heights.iter_mut() {
                        *child_height += extra_space / count;
                    }
                }
                Grow::Ratio(ref amounts) => {
                    let total: f32 = amounts.iter().sum();
                    for (i, child_height) in container_heights.iter_mut().enumerate() {
                        *child_height += (amounts[i] / total) * extra_space;
                    }
                }
            }
        }
        container_heights
    }

    fn get_child_sizes(&self) -> (Vec<f32>, Vec<f32>) {
        let mut child_widths = Vec::with_capacity(self.children.len());
        let mut child_heights = Vec::with_capacity(self.children.len());
        for child in self.children.iter() {
            let (child_width, child_height) = child.borrow().get_size();
            child_widths.push(child_width);
            child_heights.push(child_height)
        }
        (child_widths, child_heights)
    }

    fn get_layout(&self) -> (Vec<(f32, f32)>, Vec<f32>, Vec<f32>) {
        let (max_width, max_height) = self.get_max_size();
        let (child_widths, child_heights) = self.get_child_sizes();
        let width = self.get_width_from_child_widths(&child_widths, max_width);
        let container_heights = self.get_heights_from_child_heights(&child_heights, max_height);

        let mut child_offsets = Vec::with_capacity(self.children.len());
        let mut y_offset = self.style.padding;
        for i in 0..self.children.len() {
            let child_width = child_widths[i];
            let child_height = child_heights[i];
            let container_height = container_heights[i];

            let x = match self.style.h_align {
                HAlign::Left => self.style.padding,
                HAlign::Center => width / 2.0 - child_width / 2.0,
                HAlign::Right => width - self.style.padding - child_width,
            };
            let y = y_offset + match self.style.v_align {
                VAlign::Top => 0.0,
                VAlign::Center => container_height / 2.0 - child_height / 2.0,
                VAlign::Bottom => container_height - child_height,
            };
            child_offsets.push((x, y));

            y_offset += container_height + self.style.spacing;
        }

        (child_offsets, child_widths, child_heights)
    }
}

impl Widget for Column {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.container_width = width;
        self.container_height = height;
        let (max_width, max_height) = self.get_max_size();
        let min_child_heights: Vec<f32> = self.children.iter().map(|child| child.borrow().get_min_size().0).collect();
        let container_heights = self.get_heights_from_child_heights(&min_child_heights, max_height);
        for (i, child) in self.children.iter().enumerate() {
            child.borrow_mut().set_container_size(max_width, Some(container_heights[i]));
        }
    }

    fn get_min_size(&self) -> (f32, f32) {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in self.children.iter() {
            let (child_width, child_height) = child.borrow().get_min_size();
            width = width.max(child_width);
            height += child_height + self.style.spacing;
        }
        (width + 2.0 * self.style.padding, height + 2.0 * self.style.padding)
    }

    fn get_size(&self) -> (f32, f32) {
        let (max_width, max_height) = self.get_max_size();
        let (child_widths, child_heights) = self.get_child_sizes();
        let container_heights = self.get_heights_from_child_heights(&child_heights, max_height);

        let width = self.get_width_from_child_widths(&child_widths, max_width);
        let height = container_heights.iter().sum::<f32>() + 2.0 * self.style.padding + self.style.spacing * (self.children.len() - 1) as f32;
        (width, height)
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        let (child_offsets, child_widths, child_heights) = self.get_layout();

        match ev {
            InputEvent::CursorMoved { .. } | InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. } | InputEvent::MouseScroll { .. } => {
                let mouse_position = input_state.mouse_drag_origin.unwrap_or(input_state.mouse_position);

                for (i, child) in self.children.iter().enumerate() {
                    let (x, y) = child_offsets[i];
                    if x <= mouse_position.x && mouse_position.x < x + child_widths[i] && y <= mouse_position.y && mouse_position.y < y + child_heights[i] {
                        if let InputEvent::MousePress { .. } = ev {
                            self.focus = Some(i);
                        }
                        child.borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: -y }));
                        break;
                    }
                }
            },
            InputEvent::KeyPress { .. } | InputEvent::KeyRelease { .. } | InputEvent::TextInput { .. } => {
                if let Some(focus) = self.focus {
                    let (x, y) = child_offsets[focus];
                    self.children[focus].borrow_mut().handle_event(ev, input_state.translate(Point { x: -x, y: -y }));
                }
            },
        }
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let (child_offsets, child_widths, child_heights) = self.get_layout();

        let mut list = DisplayList::new();
        for (i, child) in self.children.iter().enumerate() {
            let (x, y) = child_offsets[i];
            let mut child_list = child.borrow().display(input_state.translate(Point { x: -x, y: -y }));
            child_list.translate(Point { x: x, y: y });
            list.merge(child_list);
        }

        list
    }
}

pub struct ColumnStyle {
    padding: f32,
    spacing: f32,
    min_width: Option<f32>,
    max_width: Option<f32>,
    min_height: Option<f32>,
    max_height: Option<f32>,
    h_align: HAlign,
    v_align: VAlign,
    h_fill: bool,
    v_fill: Grow,
}

impl Default for ColumnStyle {
    fn default() -> ColumnStyle {
        ColumnStyle {
            padding: 0.0,
            spacing: 0.0,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            h_align: HAlign::Left,
            v_align: VAlign::Top,
            h_fill: false,
            v_fill: Grow::None,
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

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
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
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let (width, height) = self.get_size();

        let mut color = [0.15, 0.18, 0.23, 1.0];
        if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
            if 0.0 <= mouse_drag_origin.x && mouse_drag_origin.x < width && 0.0 <= mouse_drag_origin.y && mouse_drag_origin.y < height {
                color = [0.02, 0.2, 0.6, 1.0];
            }
        } else if 0.0 <= input_state.mouse_position.x && input_state.mouse_position.x < width && 0.0 <= input_state.mouse_position.y && input_state.mouse_position.y < height {
            color = [0.3, 0.4, 0.5, 1.0];
        }


        let mut list = DisplayList::new();
        list.rect(Rect { x: 0.0, y: 0.0, w: width, h: height, color: color });
        list.merge(self.contents.borrow().display(input_state));

        list
    }
}


pub struct Label {
    text: String,
    font: Rc<Font<'static>>,
    scale: Scale,
}

impl Label {
    pub fn new(text: &str, font: Rc<Font<'static>>) -> Rc<RefCell<Label>> {
        Rc::new(RefCell::new(Label { text: text.to_string(), font: font, scale: Scale::uniform(14.0) }))
    }

    pub fn set_text(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
    }
}

impl Widget for Label {
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_min_size(&self) -> (f32, f32) {
        get_label_size(&*self.font, self.scale, &self.text)
    }

    fn get_size(&self) -> (f32, f32) {
        get_label_size(&*self.font, self.scale, &self.text)
    }

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {

    }

    fn display(&self, input_state: InputState) -> DisplayList {
        let glyphs = layout_label(&*self.font, self.scale, 0.0, 0.0, &self.text);

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
    fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {

    }

    fn get_min_size(&self) -> (f32, f32) {
        self.get_size()
    }

    fn get_size(&self) -> (f32, f32) {
        let (width, height) = get_label_size(&*self.font, self.scale, &self.text);
        (width.max(40.0), height)
    }

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


pub struct IntegerInput {
    value: i32,
    new_value: Option<i32>,
    on_change: Option<Box<Fn(i32)>>,
    container: Rc<RefCell<Container>>,
    label: Rc<RefCell<Label>>,
}

impl IntegerInput {
    pub fn new(value: i32, font: Rc<Font<'static>>) -> Rc<RefCell<IntegerInput>> {
        let label = Label::new(&value.to_string(), font);
        let container = Container::new(label.clone());
        container.borrow_mut().get_style().padding = 2.0;
        Rc::new(RefCell::new(IntegerInput { value: value, new_value: None, on_change: None, container: container, label: label }))
    }

    pub fn on_change<F>(&mut self, callback: F) where F: 'static + Fn(i32) {
        self.on_change = Some(Box::new(callback));
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

    fn handle_event(&mut self, ev: InputEvent, input_state: InputState) {
        match ev {
            InputEvent::CursorMoved { position } => {
                if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
                    let dy = -(input_state.mouse_position.y - mouse_drag_origin.y);
                    let new_value = self.value + (dy / 8.0) as i32;
                    self.new_value = Some(new_value);
                    self.label.borrow_mut().set_text(&new_value.to_string());
                    if let Some(ref on_change) = self.on_change {
                        on_change(new_value);
                    }
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
    }

    fn display(&self, input_state: InputState) -> DisplayList {
        self.container.borrow().display(input_state)
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
