use unsafe_any::UnsafeAny;

use std::any::TypeId;
use std::marker::PhantomData;
use std::f32;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;
use std::mem;
use std::ops::{Index, IndexMut};
use std::slice::IterMut;

use slab::Slab;
use priority_queue::PriorityQueue;
use std::cmp::Reverse;
use anymap::AnyMap;

use glium::glutin;
use rusttype::{Font, Scale, point, PositionedGlyph};

use render::*;

/* component */

pub trait Component {
    fn install(&self, context: &mut InstallContext<Self>, children: &[Child]) {}
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32) {
        if let Some(child) = children.get_mut(0) {
            child.layout(max_width, max_height)
        } else {
            (0.0, 0.0)
        }
    }
    fn display(&self, _width: f32, _height: f32, _list: &mut DisplayList) {}
}

trait ComponentWrapper {
    fn install(&self, ui: &mut UI, id: Id, children: &[Child]);
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32);
    fn display(&self, width: f32, height: f32, list: &mut DisplayList);
    fn get_type_id(&self) -> TypeId where Self: 'static { TypeId::of::<Self>() }
}

impl ComponentWrapper {
    fn is<T: 'static>(&self) -> bool { self.get_type_id() == TypeId::of::<T>() }

    unsafe fn downcast_ref_unchecked<T>(&self) -> &T {
        &*(self as *const ComponentWrapper as *const T)
    }

    unsafe fn downcast_mut_unchecked<T>(&mut self) -> &mut T {
        &mut *(self as *mut ComponentWrapper as *mut T)
    }
}

impl<C: Component> ComponentWrapper for C {
    fn install(&self, ui: &mut UI, id: Id, children: &[Child]) {
        self.install(&mut InstallContext { ui, id, phantom_data: PhantomData }, children);
    }
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32) {
        self.layout(max_width, max_height, children)
    }
    fn display(&self, width: f32, height: f32, list: &mut DisplayList) {
        self.display(width, height, list);
    }
}

/* ui */

type Id = usize;

pub struct UI {
    width: f32,
    height: f32,

    components: Slab<ComponentData>,
    listeners: Slab<AnyMap>,
    layout: Layout,

    under_cursor: HashSet<Id>,
    focus: Option<Id>,
    mouse_focus: Option<Id>,
    input_state: InputState,

    queue: VecDeque<QueueEntry>,
}

struct ComponentData {
    component: Box<ComponentWrapper>,
    redirect: Redirect,
    children: Vec<Id>,
}

enum Redirect {
     None,
     Inner(Id),
     Child(Id, usize),
}

impl ComponentData {
    fn new(component: Box<ComponentWrapper>) -> ComponentData {
        ComponentData {
            component,
            redirect: Redirect::None,
            children: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct Layout {
    id: Id,
    bounds: BoundingBox,
    children: Vec<Layout>,
}

impl Layout {
    fn new(id: Id) -> Layout {
        Layout {
            id: id,
            bounds: BoundingBox::new(0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
        }
    }
}

struct Listener<E> {
    id: Id,
    callback: Box<UnsafeAny>,
    dispatcher: fn(Id, &mut Box<UnsafeAny>, &mut Slab<ComponentData>, &mut VecDeque<QueueEntry>, E),
}

struct QueueEntry {
    callback: fn(&mut UI, Id, Box<UnsafeAny>),
    id: Id,
    event: Box<UnsafeAny>,
}

impl UI {
    pub fn new(width: f32, height: f32) -> UI {
        let mut ui = UI {
            width: width,
            height: height,

            components: Slab::new(),
            listeners: Slab::new(),
            layout: Layout::new(0),

            under_cursor: HashSet::new(),
            focus: None,
            mouse_focus: None,
            input_state: InputState::default(),

            queue: VecDeque::new(),
        };

        ui.component(Box::new(Empty));

        ui
    }

    /* size */

    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.layout();
    }

    /* tree */

    pub fn place<C: Component + 'static>(&mut self, component: C) {
        let root: Slot<Empty> = Slot { ui: self, owner: 0, id: 0, phantom_data: PhantomData };
        root.place(component);
    }

    fn component(&mut self, component: Box<ComponentWrapper>) -> Id {
        let id = self.components.insert(ComponentData::new(component));
        self.listeners.insert(AnyMap::new());
        id
    }

    fn cleanup(&mut self, id: Id) {
        self.listeners[id] = AnyMap::new();
        for child in mem::replace(&mut self.components[id].children, Vec::new()) {
            self.cleanup(child);
            self.components.remove(child);
            self.listeners.remove(child);
        }
        if let Redirect::Inner(inner) = self.components[id].redirect {
            self.cleanup(inner);
            self.components.remove(inner);
            self.listeners.remove(inner);
        }
    }

    /* event handling */

    pub fn modifiers(&mut self, modifiers: KeyboardModifiers) {
        self.input_state.modifiers = modifiers;
    }

    pub fn input(&mut self, event: InputEvent) -> UIEventResponse {
        match event {
            InputEvent::MouseMove(position) => {
                self.input_state.mouse_position = position;
            }
            InputEvent::MousePress(button) => {
                match button {
                    MouseButton::Left => {
                        self.input_state.mouse_left_pressed = true;
                    }
                    MouseButton::Middle => {
                        self.input_state.mouse_middle_pressed = true;
                    }
                    MouseButton::Right => {
                        self.input_state.mouse_right_pressed = true;
                    }
                }
            }
            InputEvent::MouseRelease(button) => {
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

        let path = self.trace(self.input_state.mouse_position);

        let mut ui_response: UIEventResponse = Default::default();

        let handler = match event {
            InputEvent::MouseMove(..) | InputEvent::MousePress(..) | InputEvent::MouseRelease(..) | InputEvent::MouseScroll(..) => {
                if let Some(mouse_focus) = self.mouse_focus {
                    self.fire_input_event(mouse_focus, event);
                } else {
                    for component in path.iter().rev() {
                        if self.fire_input_event(*component, event) {
                            break;
                        }
                    }
                }
            },
            InputEvent::KeyPress(..) | InputEvent::KeyRelease(..) | InputEvent::TextInput(..) => {
                if let Some(focus) = self.focus {
                    self.fire_input_event(focus, event);
                } else {
                    self.fire_input_event(0, event);
                }
            }
        };

        self.mouse_enter_leave(&path);

        ui_response
    }

    fn trace(&self, mouse_position: Point) -> Vec<Id> {
        let mut under_cursor = Vec::new();
        self.trace_inner(mouse_position, &self.layout, &mut under_cursor);
        under_cursor
    }

    fn trace_inner(&self, mouse_position: Point, layout: &Layout, under_cursor: &mut Vec<Id>) -> bool {
        if layout.bounds.contains_point(mouse_position) {
            under_cursor.push(layout.id);
            let mouse_adjusted = mouse_position - layout.bounds.pos;
            for child in layout.children.iter() {
                if self.trace_inner(mouse_adjusted, child, under_cursor) {
                    return true;
                }
            }
        }
        false
    }

    fn mouse_enter_leave(&mut self, path: &[Id]) {
        let old_under_cursor = mem::replace(&mut self.under_cursor, HashSet::new());

        let mut new_under_cursor = HashSet::new();
        new_under_cursor.extend(path.iter());

        for new in new_under_cursor.difference(&old_under_cursor) {
            self.fire_event(*new, MouseEnter);
        }
        for old in old_under_cursor.difference(&new_under_cursor) {
            self.fire_event(*old, MouseLeave);
        }

        self.under_cursor = new_under_cursor;
    }

    fn fire_event<E: 'static>(&mut self, id: Id, event: E) -> bool {
        if let Some(listener) = self.listeners[id].get_mut::<Listener<E>>() {
            (listener.dispatcher)(listener.id, &mut listener.callback, &mut self.components, &mut self.queue, event);
            true
        } else {
            false
        }
    }

    fn fire_input_event(&mut self, id: Id, event: InputEvent) -> bool {
        match event {
            InputEvent::MouseMove(position) => self.fire_event(id, MouseMove(position)),
            InputEvent::MousePress(button) => self.fire_event(id, MousePress(button)),
            InputEvent::MouseRelease(button) => self.fire_event(id, MouseRelease(button)),
            InputEvent::MouseScroll(delta) => self.fire_event(id, MouseScroll(delta)),
            InputEvent::KeyPress(button) => self.fire_event(id, KeyPress(button)),
            InputEvent::KeyRelease(button) => self.fire_event(id, KeyRelease(button)),
            InputEvent::TextInput(c) => self.fire_event(id, TextInput(c)),
        }
    }

    fn drain_queue(&mut self) {
        while let Some(entry) = self.queue.pop_front() {
            (entry.callback)(self, entry.id, entry.event);
        }
    }

    /* display */

    pub fn display(&mut self) -> DisplayList {
        self.update();
        self.layout();
        let mut list = DisplayList::new();
        self.display_component(&self.layout, &mut list);
        list
    }

    fn display_component(&self, layout: &Layout, list: &mut DisplayList) {
        list.push_translate(layout.bounds.pos);
        self.components[layout.id].component.display(layout.bounds.size.x, layout.bounds.size.y, list);
        for child in layout.children.iter() {
            self.display_component(child, list);
        }
        list.pop_translate();
    }

    /* layout */

    fn layout(&mut self) {
        self.layout = Layout::new(find_leaf(&self.components, 0));
        LayoutChild { components: &self.components, layout: &mut self.layout }
            .layout(self.width, self.height);
        // println!("{:#?}", &self.layout);
    }

    /* update */

    fn update(&mut self) {
        self.drain_queue();
        self.update_component(0);
    }

    fn update_component(&mut self, id: Id) {
        if let Redirect::Child(..) = self.components[id].redirect {
            return;
        }
        let mut component = mem::replace(&mut self.components[id].component, Box::new(Empty));
        let children: Vec<Child> = (0..self.components[id].children.len()).map(|i| Child { parent: id, child: i }).collect();
        component.install(self, id, &children);
        self.components[id].component = component;
        for child in self.components[id].children.clone() {
            self.update_component(child);
        }
        if let Redirect::Inner(inner) = self.components[id].redirect {
            self.update_component(inner);
        }
    }
}

fn find_leaf(components: &Slab<ComponentData>, id: Id) -> Id {
    let mut id = id;
    loop {
        match components[id].redirect {
            Redirect::Inner(redirect) => { id = redirect; }
            Redirect::Child(parent, child) => { id = components[parent].children[child]; }
            Redirect::None => { break; }
        }
    }
    id
}

/* install */

pub struct InstallContext<'a, C: Component + ?Sized> {
    ui: &'a mut UI,
    id: Id,
    phantom_data: PhantomData<C>,
}

impl<'a, C: Component> InstallContext<'a, C> {
    pub fn root<'b>(&'b mut self) -> Slot<'b, C> {
        if let Redirect::Inner(inner) = self.ui.components[self.id].redirect {
            Slot { ui: self.ui, owner: self.id, id: inner, phantom_data: PhantomData }
        } else {
            let inner = self.ui.component(Box::new(Empty));
            self.ui.components[self.id].redirect = Redirect::Inner(inner);
            Slot { ui: self.ui, owner: self.id, id: inner, phantom_data: PhantomData }
        }
    }
}

pub struct Child {
    parent: Id,
    child: usize,
}

pub struct Slot<'a, C> {
    ui: &'a mut UI,
    owner: Id,
    id: Id,
    phantom_data: PhantomData<C>,
}

impl<'a, C: Component> Slot<'a, C> {
    pub fn get<D: Component + 'static>(self) -> Option<ComponentRef<'a, C, D>> {
        if self.ui.components[self.id].component.is::<D>() {
            Some(ComponentRef { ui: self.ui, owner: self.owner, id: self.id, child_index: 0, phantom_data: PhantomData })
        } else {
            None
        }
    }

    pub fn place<D: Component + 'static>(self, component: D) -> ComponentRef<'a, C, D> {
        self.ui.cleanup(self.id);
        self.ui.components[self.id] = ComponentData::new(Box::new(component));
        ComponentRef { ui: self.ui, owner: self.owner, id: self.id, child_index: 0, phantom_data: PhantomData }
    }

    pub fn get_or_place<D: Component + 'static, F: FnOnce() -> D>(self, f: F) -> ComponentRef<'a, C, D> {
        if self.ui.components[self.id].component.is::<D>() {
            ComponentRef { ui: self.ui, owner: self.owner, id: self.id, child_index: 0, phantom_data: PhantomData }
        } else {
            self.place(f())
        }
    }

    pub fn place_child(self, child: Child) {
        let mut component = ComponentData::new(Box::new(Empty));
        component.redirect = Redirect::Child(child.parent, child.child);
        self.ui.cleanup(self.id);
        self.ui.components[self.id] = component;
    }
}

pub struct ComponentRef<'a, C: Component, D: Component> {
    ui: &'a mut UI,
    owner: Id,
    id: Id,
    child_index: usize,
    phantom_data: PhantomData<(C, D)>,
}

impl<'a, C: Component, D: Component> ComponentRef<'a, C, D> {
    pub fn get(&self) -> &D {
        unsafe { self.ui.components[self.id].component.downcast_ref_unchecked() }
    }

    pub fn get_mut(&mut self) -> &mut D {
        unsafe { self.ui.components[self.id].component.downcast_mut_unchecked() }
    }

    pub fn child<'b>(&'b mut self) -> Slot<'b, C> {
        let id = if self.child_index == self.ui.components[self.id].children.len() {
            let id = self.ui.component(Box::new(Empty));
            self.ui.components[self.id].children.push(id);
            id
        } else {
            self.ui.components[self.id].children[self.child_index]
        };
        self.child_index += 1;
        Slot { ui: self.ui, owner: self.owner, id, phantom_data: PhantomData }
    }

    pub fn listen<E: 'static, F: Fn(&mut EventContext<C>, E) + 'static>(&mut self, callback: F) {
        self.ui.listeners[self.id].insert::<Listener<E>>(Listener {
            id: self.owner,
            callback: Box::new(callback),
            dispatcher: |id, callback, components, queue, event| {
                let f: &mut F = unsafe { callback.downcast_mut_unchecked() };
                f(&mut EventContext { id, components, queue, phantom_data: PhantomData }, event);
            },
        });
    }
}

/* events */

pub struct EventContext<'a, C: Component> {
    id: Id,
    components: &'a mut Slab<ComponentData>,
    queue: &'a mut VecDeque<QueueEntry>,
    phantom_data: PhantomData<C>,
}

impl<'a, C: Component> EventContext<'a, C> {
    pub fn get(&self) -> &C {
        unsafe { self.components[self.id].component.downcast_ref_unchecked() }
    }

    pub fn get_mut(&mut self) -> &mut C {
        unsafe { self.components[self.id].component.downcast_mut_unchecked() }
    }

    pub fn fire<E: 'static>(&mut self, event: E) {
        let id = self.id;
        self.queue.push_back(QueueEntry {
            id: self.id,
            callback: |ui, id, event| {
                ui.fire_event::<E>(id, *unsafe { event.downcast_unchecked() });
            },
            event: Box::new(event),
        });
    }
}

/* layout */

pub struct LayoutChild<'a> {
    components: &'a Slab<ComponentData>,
    layout: &'a mut Layout,
}

impl<'a> LayoutChild<'a> {
    pub fn layout(&mut self, max_width: f32, max_height: f32) -> (f32, f32) {
        self.layout.children = self.components[self.layout.id].children.iter()
            .map(|id| Layout::new(find_leaf(self.components, *id))).collect();
        let components = &self.components;
        let mut children: Vec<LayoutChild> = self.layout.children.iter_mut().map(|layout| LayoutChild { components, layout }).collect();
        let (width, height) = self.components[self.layout.id].component.layout(max_width, max_height, &mut children);
        self.layout.bounds.size = Point::new(width, height);
        (width, height)
    }

    pub fn offset(&mut self, x: f32, y: f32) {
        self.layout.bounds.pos = Point::new(x, y);
    }
}


struct Empty;

impl Component for Empty {}


pub struct BackgroundColor {
    color: [f32; 4],
}

impl BackgroundColor {
    pub fn new(color: [f32; 4]) -> BackgroundColor {
        BackgroundColor { color }
    }

    pub fn color(&mut self, color: [f32; 4]) {
        self.color = color;
    }
}

impl Component for BackgroundColor {
    fn display(&self, width: f32, height: f32, list: &mut DisplayList) {
        list.rect(Rect { x: 0.0, y: 0.0, w: width, h: height, color: self.color });
    }
}


pub struct Container {
    max_width: f32,
    max_height: f32,
}

impl Container {
    pub fn new(max_width: f32, max_height: f32) -> Container {
        Container { max_width, max_height }
    }

    pub fn max_width(&mut self, max_width: f32) {
        self.max_width = max_width;
    }

    pub fn max_height(&mut self, max_height: f32) {
        self.max_height = max_height;
    }
}

impl Component for Container {
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32) {
        if let Some(child) = children.get_mut(0) {
            child.layout(self.max_width.min(max_width), self.max_height.min(max_height))
        } else {
            (0.0, 0.0)
        }
    }
}


pub struct Padding {
    padding: f32,
}

impl Padding {
    pub fn new(padding: f32) -> Padding {
        Padding { padding }
    }

    pub fn padding(&mut self, padding: f32) {
        self.padding = padding;
    }
}

impl Component for Padding {
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32) {
        if let Some(child) = children.get_mut(0) {
            let (child_width, child_height) = child.layout(max_width - 2.0 * self.padding, max_height - 2.0 * self.padding);
            child.offset(self.padding, self.padding);
            (child_width + 2.0 * self.padding, child_height + 2.0 * self.padding)
        } else {
            (2.0 * self.padding, 2.0 * self.padding)
        }
    }
}


pub struct Row {
    spacing: f32,
}

impl Row {
    pub fn new(spacing: f32) -> Row {
        Row { spacing }
    }

    pub fn spacing(&mut self, spacing: f32) {
        self.spacing = spacing;
    }
}

impl Component for Row {
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32) {
        let mut x: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in children {
            let (child_width, child_height) = child.layout(max_width - x, max_height);
            child.offset(x, 0.0);
            x += child_width + self.spacing;
            height = height.max(child_height);
        }
        (x - self.spacing, height)
    }
}


pub struct Col {
    spacing: f32,
}

impl Col {
    pub fn new(spacing: f32) -> Col {
        Col { spacing }
    }

    pub fn spacing(&mut self, spacing: f32) {
        self.spacing = spacing;
    }
}

impl Component for Col {
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32) {
        let mut width: f32 = 0.0;
        let mut y: f32 = 0.0;
        for child in children {
            let (child_width, child_height) = child.layout(max_width, max_height - y);
            child.offset(0.0, y);
            width = width.max(child_width);
            y += child_height + self.spacing;
        }
        (width, y - self.spacing)
    }
}


pub struct TextStyle {
    pub font: Font<'static>,
    pub scale: Scale,
}

pub struct Text {
    text: String,
    style: TextStyle,
    glyphs: RefCell<Vec<PositionedGlyph<'static>>>,
}

impl Text {
    pub fn new(text: String, style: TextStyle) -> Text {
        Text { text, style, glyphs: RefCell::new(Vec::new()) }
    }

    pub fn text(&mut self, text: String) {
        self.text = text;
    }

    fn layout_text(&self, max_width: f32) -> (f32, f32) {
        use unicode_normalization::UnicodeNormalization;

        let mut glyphs = self.glyphs.borrow_mut();
        glyphs.clear();
        let mut wrapped = false;

        let v_metrics = self.style.font.v_metrics(self.style.scale);
        let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let mut caret = point(0.0, v_metrics.ascent);
        let mut last_glyph_id = None;
        for c in self.text.nfc() {
            if c.is_control() {
                match c {
                    '\r' => {
                        caret = point(0.0, caret.y + advance_height);
                    }
                    '\n' => {},
                    _ => {}
                }
                continue;
            }
            let base_glyph = if let Some(glyph) = self.style.font.glyph(c) {
                glyph
            } else {
                continue;
            };
            if let Some(id) = last_glyph_id.take() {
                caret.x += self.style.font.pair_kerning(self.style.scale, id, base_glyph.id());
            }
            last_glyph_id = Some(base_glyph.id());
            let mut glyph = base_glyph.scaled(self.style.scale).positioned(caret);
            if let Some(bb) = glyph.pixel_bounding_box() {
                if bb.max.x > (max_width) as i32 {
                    wrapped = true;
                    caret = point(0.0, caret.y + advance_height);
                    glyph = glyph.into_unpositioned().positioned(caret);
                    last_glyph_id = None;
                }
            }
            caret.x += glyph.unpositioned().h_metrics().advance_width;
            glyphs.push(glyph.standalone());
        }

        let width = if wrapped { max_width } else { caret.x };
        (width, caret.y - v_metrics.descent)
    }
}

impl Component for Text {
    fn layout(&self, max_width: f32, max_height: f32, children: &mut [LayoutChild]) -> (f32, f32) {
        self.layout_text(max_width)
    }

    fn display(&self, width: f32, height: f32, list: &mut DisplayList) {
        for glyph in self.glyphs.borrow().iter() {
            let position = glyph.position();
            list.glyph(glyph.clone().into_unpositioned().positioned(point(position.x, position.y)));
        }
    }
}


#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum ButtonState {
    Up,
    Hover,
    Down,
}

pub struct Button {
    state: ButtonState,
}

pub struct ClickEvent;

impl Button {
    pub fn new() -> Button {
        Button { state: ButtonState::Up }
    }
}

impl Component for Button {
    fn install(&self, context: &mut InstallContext<Button>, children: &[Child]) {
        let color = match self.state {
            ButtonState::Up => [0.15, 0.18, 0.23, 1.0],
            ButtonState::Hover => [0.3, 0.4, 0.5, 1.0],
            ButtonState::Down => [0.02, 0.2, 0.6, 1.0],
        };

        let mut bg = context.root().place(BackgroundColor::new(color));
        bg.listen(|ctx, e: MousePress| {
            ctx.fire(ClickEvent);
            ctx.get_mut().state = ButtonState::Down;
        });
        bg.child().place(Padding::new(10.0));
    }

    // fn handle(&mut self, ctx: &mut ComponentContext<Button>, event: ElementEvent) {
    //     match event {
    //         ElementEvent::MouseEnter => {
    //             ctx.set(self.state, ButtonState::Hover);
    //         }
    //         ElementEvent::MouseLeave => {
    //             ctx.set(self.state, ButtonState::Up);
    //         }
    //         ElementEvent::MousePress(MouseButton::Left) => {
    //             ctx.set(self.state, ButtonState::Down);
    //         }
    //         ElementEvent::MouseRelease(MouseButton::Left) => {
    //             if *ctx.get(self.state) == ButtonState::Down {
    //                 ctx.set(self.state, ButtonState::Hover);
    //                 ctx.fire::<ClickEvent>(ClickEvent);
    //             }
    //         }
    //         _ => {}
    //     }
    // }
}


// pub struct Stack;

// impl Stack {
//     pub fn install(mut ctx: Context<Stack>) -> Stack {
//         let id = ctx.get_self();
//         ctx.register_slot(id);
//         Stack
//     }

//     fn main_cross(&self, axis: Axis, point: Point) -> (f32, f32) { match axis { Axis::Horizontal => (point.x, point.y), Axis::Vertical => (point.y, point.x) } }
//     fn x_y(&self, axis: Axis, main: f32, cross: f32) -> Point { match axis { Axis::Horizontal => Point::new(main, cross), Axis::Vertical => Point::new(cross, main) } }
// }

// style! {
//     struct StackStyle {
//         spacing: f32,
//         axis: Axis,
//         grow: Grow,
//     },
//     StackStylePatch
// }

// impl Default for StackStyle {
//     fn default() -> StackStyle {
//         StackStyle {
//             spacing: 0.0,
//             axis: Axis::Horizontal,
//             grow: Grow::None,
//         }
//     }
// }

// impl Element for Stack {
//     fn measure(&self, resources: &Resources, children: &[BoundingBox]) -> BoundingBox {
//         let box_style = resources.get_style::<BoxStyle>();
//         let stack_style = resources.get_style::<StackStyle>();

//         let mut main = 0.0f32;
//         let mut cross = 0.0f32;
//         for child_box in children {
//             let (child_main, child_cross) = self.main_cross(stack_style.axis, child_box.size);
//             main += child_main;
//             cross = cross.max(child_cross);
//         }

//         main += 2.0 * box_style.padding + stack_style.spacing * (children.len() as i32 - 1).max(0) as f32;
//         cross += 2.0 * box_style.padding;

//         let mut size = self.x_y(stack_style.axis, main, cross);
//         size.x = size.x.max(box_style.min_width).min(box_style.max_width);
//         size.y = size.y.max(box_style.min_height).min(box_style.max_height);

//         BoundingBox { pos: Point::new(0.0, 0.0), size: size }
//     }

//     fn arrange(&mut self, resources: &Resources, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
//         let box_style = resources.get_style::<BoxStyle>();
//         let stack_style = resources.get_style::<StackStyle>();

//         let (main_offset, cross_offset) = self.main_cross(stack_style.axis, bounds.pos);
//         let (main_max, cross_max) = self.main_cross(stack_style.axis, bounds.size);
//         let mut children_main = 0.0;
//         for child_box in children.iter() {
//             children_main += self.main_cross(stack_style.axis, child_box.size).0;
//         }
//         let extra = main_max - 2.0 * box_style.padding - stack_style.spacing * (children.len() as i32 - 1).max(0) as f32 - children_main;
//         let child_cross = cross_max - 2.0 * box_style.padding;

//         match stack_style.grow {
//             Grow::None => {
//                 for child_box in children.iter_mut() {
//                     let (child_main, _child_cross) = self.main_cross(stack_style.axis, child_box.size);
//                     child_box.size = self.x_y(stack_style.axis, child_main, child_cross);
//                 }
//             }
//             Grow::Equal => {
//                 let children_len = children.len() as f32;
//                 for child_box in children.iter_mut() {
//                     let (child_main, _child_cross) = self.main_cross(stack_style.axis, child_box.size);
//                     child_box.size = self.x_y(stack_style.axis, child_main + extra / children_len, child_cross);
//                 }
//             }
//             Grow::Ratio(amounts) => {
//                 let total: f32 = amounts.iter().sum();
//                 for (i, child_box) in children.iter_mut().enumerate() {
//                     let (child_main, _child_cross) = self.main_cross(stack_style.axis, child_box.size);
//                     child_box.size = self.x_y(stack_style.axis, child_main + extra * amounts[i] / total, child_cross);
//                 }
//             }
//         }

//         let mut main_offset = main_offset + box_style.padding;
//         let cross_offset = cross_offset + box_style.padding;
//         for child_box in children.iter_mut() {
//             let child_main = self.main_cross(stack_style.axis, child_box.size).0;
//             child_box.pos = self.x_y(stack_style.axis, main_offset, cross_offset);
//             child_box.size = self.x_y(stack_style.axis, child_main, child_cross);
//             main_offset += child_main + stack_style.spacing;
//         }

//         bounds
//     }

//     fn display(&self, resources: &Resources, bounds: BoundingBox, input_state: InputState, list: &mut DisplayList) {
//         let box_style = resources.get_style::<BoxStyle>();

//         // let color = [0.15, 0.18, 0.23, 1.0];
//         list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: box_style.color });
//     }
// }

// #[derive(Copy, Clone, PartialEq)]
// pub enum Align {
//     Beginning,
//     Center,
//     End
// }

// #[derive(Copy, Clone, PartialEq)]
// pub enum Axis {
//     Horizontal,
//     Vertical,
// }

// #[derive(Clone, PartialEq)]
// pub enum Grow {
//     None,
//     Equal,
//     Ratio(Vec<f32>),
// }


// pub struct Textbox {
//     label: Rc<RefCell<Label>>,
//     on_change: Option<Box<Fn(&str)>>,
// }

// impl Textbox {
//     pub fn new(font: Rc<Font<'static>>) -> Rc<RefCell<Textbox>> {
//         Rc::new(RefCell::new(Textbox { label: Label::new("", font), on_change: None }))
//     }

//     pub fn on_change<F>(&mut self, callback: F) where F: 'static + Fn(&str) {
//         self.on_change = Some(Box::new(callback));
//     }
// }

// impl Element for Textbox {
//     fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
//         self.label.borrow_mut().set_container_size(width, height);
//     }

//     fn get_min_size(&self) -> (f32, f32) {
//         self.label.borrow().get_size()
//     }

//     fn get_size(&self) -> (f32, f32) {
//         let (width, height) = self.label.borrow().get_size();
//         (width.max(40.0), height)
//     }

//     fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
//         match ev {
//             InputEvent::KeyPress { button: KeyboardButton::Back } => {
//                 let mut label = self.label.borrow_mut();
//                 label.modify_text(|text| { text.pop(); () });
//                 if let Some(ref on_change) = self.on_change {
//                     on_change(label.get_text());
//                 }
//             }
//             InputEvent::TextInput { character: c } => {
//                 if !c.is_control() {
//                     let mut label = self.label.borrow_mut();
//                     label.modify_text(move |text| text.push(c));
//                     if let Some(ref on_change) = self.on_change {
//                         on_change(label.get_text());
//                     }
//                 }
//             }
//             _ => {}
//         }

//         Default::default()
//     }

//     fn display(&self, input_state: InputState, list: &mut DisplayList) {
//         let color = [0.1, 0.15, 0.2, 1.0];
//         let (width, height) = self.get_size();
//         list.rect(Rect { x: 0.0, y: 0.0, w: width.max(40.0), h: height, color: color });

//         self.label.borrow().display(input_state, list);
//     }
// }


struct InputState {
    mouse_position: Point,
    mouse_left_pressed: bool,
    mouse_middle_pressed: bool,
    mouse_right_pressed: bool,
    modifiers: KeyboardModifiers,
}

impl Default for InputState {
    fn default() -> InputState {
        InputState {
            mouse_position: Point { x: -1.0, y: -1.0 },
            mouse_left_pressed: false,
            mouse_middle_pressed: false,
            mouse_right_pressed: false,
            modifiers: KeyboardModifiers::default(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum InputEvent {
    MouseMove(Point),
    MousePress(MouseButton),
    MouseRelease(MouseButton),
    MouseScroll(f32),
    KeyPress(KeyboardButton),
    KeyRelease(KeyboardButton),
    TextInput(char),
}

#[derive(Copy, Clone, Debug)]
pub struct MouseEnter;

#[derive(Copy, Clone, Debug)]
pub struct MouseLeave;

#[derive(Copy, Clone, Debug)]
pub struct MouseMove(Point);

#[derive(Copy, Clone, Debug)]
pub struct MousePress(MouseButton);

#[derive(Copy, Clone, Debug)]
pub struct MouseRelease(MouseButton);

#[derive(Copy, Clone, Debug)]
pub struct MouseScroll(f32);

#[derive(Copy, Clone, Debug)]
pub struct KeyPress(KeyboardButton);

#[derive(Copy, Clone, Debug)]
pub struct KeyRelease(KeyboardButton);

#[derive(Copy, Clone, Debug)]
pub struct TextInput(char);

#[derive(Copy, Clone)]
pub struct UIEventResponse {
    pub mouse_position: Option<Point>,
    pub mouse_cursor: Option<MouseCursor>,
    pub hide_cursor: Option<bool>,
}

impl Default for UIEventResponse {
    fn default() -> UIEventResponse {
        UIEventResponse {
            mouse_position: None,
            mouse_cursor: None,
            hide_cursor: None,
        }
    }
}

impl UIEventResponse {
    fn merge(&mut self, other: UIEventResponse) {
        self.mouse_position = self.mouse_position.or(other.mouse_position);
        self.mouse_cursor = self.mouse_cursor.or(other.mouse_cursor);
        self.hide_cursor = self.hide_cursor.or(other.hide_cursor);
    }
}

#[derive(Copy, Clone)]
pub struct KeyboardModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Default for KeyboardModifiers {
    fn default() -> KeyboardModifiers {
        KeyboardModifiers {
            shift: false,
            ctrl: false,
            alt: false,
            logo: false,
        }
    }
}

impl KeyboardModifiers {
    pub fn from_glutin(modifiers: glutin::ModifiersState) -> KeyboardModifiers {
        KeyboardModifiers {
            shift: modifiers.shift,
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            logo: modifiers.logo,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Copy, Clone, Debug)]
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

#[allow(dead_code)]
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

#[derive(Copy, Clone, Debug)]
pub struct Point { pub x: f32, pub y: f32 }

impl Point {
    pub fn new(x: f32, y: f32) -> Point {
        Point { x: x, y: y }
    }
}

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

#[derive(Copy, Clone, Debug)]
pub struct BoundingBox { pub pos: Point, pub size: Point }

impl BoundingBox {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> BoundingBox {
        BoundingBox { pos: Point { x: x, y: y }, size: Point { x: w, y: h } }
    }

    pub fn contains_point(&self, point: Point) -> bool {
        point.x > self.pos.x && point.x < self.pos.x + self.size.x && point.y > self.pos.y && point.y < self.pos.y + self.size.y
    }
}
