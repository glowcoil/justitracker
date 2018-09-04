use std::collections::{HashMap, HashSet};

use anymap::AnyMap;
use unsafe_any::UnsafeAny;

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::mem;
use std::borrow::BorrowMut;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::f32;

use slab::Slab;

use glium::glutin;
use rusttype::{FontCollection, Font, Scale, point, PositionedGlyph};

use render::*;

pub struct Tree(Box<Install>);

trait Install {
    fn install(self: Box<Self>, ui: &mut UI) -> usize;
}

/* component */

pub struct Component<C> {
    component: Box<C>,
    template: Box<Fn(ComponentRef<C>) -> Tree>,
    listeners: AnyMap,
}

impl<C> Install for Component<C> where C: 'static {
    fn install(self: Box<Self>, ui: &mut UI) -> usize {
        let component = *self;
        let component_ref = ui.component(component.component);
        let Tree(installer) = (component.template)(component_ref);
        installer.install(ui)
    }
}

impl<C> From<Component<C>> for Tree where C: 'static {
    fn from(component: Component<C>) -> Self {
        Tree(Box::new(component))
    }
}

pub struct ComponentRef<C> {
    index: usize,
    phantom_data: PhantomData<C>,
}

impl<C> ComponentRef<C> {
    fn new(index: usize) -> ComponentRef<C> {
        ComponentRef {
            index: index,
            phantom_data: PhantomData,
        }
    }
}

pub fn component<C, F, T>(component: C, template: F) -> Component<C> where F: 'static + Fn(ComponentRef<C>) -> Tree {
    Component {
        component: Box::new(component),
        template: Box::new(template),
        listeners: AnyMap::new(),
    }
}

impl<C> Component<C> {
    fn on<D: 'static, E>(&mut self, listener: ComponentRef<D>, callback: impl Fn(&mut D, E) + 'static) -> &mut Component<C> {
        self.listeners.insert(Box::new(move |ui: &mut UI, event: E| {
            let listener = &mut ui.components[listener.index];
            callback(unsafe { listener.downcast_mut_unchecked() }, event);
        }));
        self
    }
}

/* element */

pub trait Element {
    type Model;

    fn measure(&self, model: &Self::Model, children: &[BoundingBox]) -> BoundingBox {
        let mut width = 0.0f32;
        let mut height = 0.0f32;
        for child_box in children {
            width = width.max(child_box.size.x);
            height = height.max(child_box.size.y);
        }

        BoundingBox { pos: Point::new(0.0, 0.0), size: Point::new(width, height) }
    }
    fn arrange(&self, model: &Self::Model, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
        for child_box in children.iter_mut() {
            *child_box = bounds;
        }

        bounds
    }
    fn display(&self, model: &Self::Model, bounds: BoundingBox, list: &mut DisplayList) {}
}

pub trait ElementDelegate {
    fn measure(&self, ui: &UI, children: &[BoundingBox]) -> BoundingBox;
    fn arrange(&self, ui: &UI, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox;
    fn display(&self, ui: &UI, bounds: BoundingBox, list: &mut DisplayList);
}

pub struct BoundElement<E: Element> {
    element: E,
    model: Reference<E::Model>,
    listeners: Vec<Box<Fn(&mut UI, InputEvent)>>,
}

impl<E> Install for BoundElement<E> where E: 'static + Element {
    fn install(self: Box<Self>, ui: &mut UI) -> usize {
        ui.element(self)
    }
}

impl<E> From<BoundElement<E>> for Tree where E: 'static + Element {
    fn from(element: BoundElement<E>) -> Self {
        Tree(Box::new(element))
    }
}

impl<E> ElementDelegate for BoundElement<E> where E: Element {
    fn measure(&self, ui: &UI, children: &[BoundingBox]) -> BoundingBox {
        let value = self.model.get(ui);
        self.element.measure(value, children)
    }
    fn arrange(&self, ui: &UI, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
        let value = self.model.get(ui);
        self.element.arrange(value, bounds, children)
    }
    fn display(&self, ui: &UI, bounds: BoundingBox, list: &mut DisplayList) {
        let value = self.model.get(ui);
        self.element.display(value, bounds, list);
    }
}

pub fn element<E>(element: E, model: Reference<E::Model>) -> BoundElement<E> where E: Element {
    BoundElement {
        element: element,
        model: model,
        listeners: Vec::new(),
    }
}

impl<E> BoundElement<E> where E: Element {
    fn on<C: 'static>(&mut self, listener: ComponentRef<C>, callback: impl Fn(&mut C, InputEvent) + 'static) -> &mut BoundElement<E> {
        self.listeners.push(Box::new(move |ui, event| {
            let listener = &mut ui.components[listener.index];
            callback(unsafe { listener.downcast_mut_unchecked() }, event);
        }));
        self
    }
}

/* element with children */

struct ElementWithChildren<E: Element> {
    element: BoundElement<E>,
    children: Vec<Tree>,
}

impl<E> Install for ElementWithChildren<E> where E: 'static + Element {
    fn install(self: Box<Self>, ui: &mut UI) -> usize {
        let element = *self;
        let index = ui.element(Box::new(element.element));
        for Tree(child) in element.children.into_iter() {
            child.install(ui);
        }
        index
    }
}

impl<E> From<ElementWithChildren<E>> for Tree where E: 'static + Element {
    fn from(element: ElementWithChildren<E>) -> Self {
        Tree(Box::new(element))
    }
}

/* property */

pub struct Property<A> {
    value: A,
}

pub struct Reference<A>(ReferenceType<A>);

enum ReferenceType<A> {
    Value(A),
    Getter(fn(&UI) -> &A),
}

impl<A> Reference<A> {
    pub fn value(value: A) -> Reference<A> {
        Reference(ReferenceType::Value(value))
    }

    fn get<'a>(&'a self, ui: &'a UI) -> &'a A {
        let Reference(ref inner) = *self;
        match inner {
            ReferenceType::Value(value) => &value,
            ReferenceType::Getter(f) => f(ui),
        }
    }
}

/* ui */

pub struct UI {
    width: f32,
    height: f32,

    components: Slab<Box<UnsafeAny>>,

    root: usize,
    elements: Slab<Box<ElementDelegate>>,
    parents: Slab<usize>,
    children: Slab<Vec<usize>>,
    layout: Slab<BoundingBox>,

    input_state: InputState,
}

impl UI {
    pub fn new(width: f32, height: f32) -> UI {
        let mut ui = UI {
            width: width,
            height: height,

            components: Slab::new(),

            root: 0,
            elements: Slab::new(),
            parents: Slab::new(),
            children: Slab::new(),
            layout: Slab::new(),

            input_state: InputState::default(),
        };

        ui
    }

    /* size */

    pub fn get_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.layout();
    }

    /* tree */

    pub fn root<T>(&mut self, tree: T) where T: Into<Tree> {
        let Tree(installer) = tree.into();
        self.root = installer.install(self);
        self.layout();
    }

    fn component<C: 'static>(&mut self, component: Box<C>) -> ComponentRef<C> {
        let index = self.components.insert(component);
        ComponentRef::new(index)
    }

    fn element<E: 'static + ElementDelegate>(&mut self, element: Box<E>) -> usize {
        let index = self.elements.insert(element);
        self.children.insert(Vec::new());
        index
    }

    fn add_child(&mut self, parent: usize, child: usize) {
        self.parents[child] = parent;
        self.children[parent].push(child);
    }
    
    /* event handling */

    pub fn set_modifiers(&mut self, modifiers: KeyboardModifiers) {
        self.input_state.modifiers = modifiers;
    }

    pub fn input(&mut self, ev: InputEvent) -> UIEventResponse {
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

        let mut ui_response: UIEventResponse = Default::default();

        let handler = match ev {
            InputEvent::CursorMoved { .. } | InputEvent::MousePress { .. } | InputEvent::MouseRelease { .. } | InputEvent::MouseScroll { .. } => {
                // if let Some(dragging) = self.dragging {
                //     Some(dragging)
                // } else {
                    let position = self.input_state.mouse_drag_origin.unwrap_or(self.input_state.mouse_position);
                    if self.layout[self.root].contains_point(position) {
                        Some(self.find_element(self.root, position))
                    } else {
                        None
                    }
                // }
            },
            InputEvent::KeyPress { .. } | InputEvent::KeyRelease { .. } | InputEvent::TextInput { .. } => {
                // self.focus.or(Some(self.root))
                Some(self.root)
            }
            _ => {
                Some(self.root)
            }
        };

        // if let Some(handler) = handler {
        //     let mut handler = handler;
        //     while self.parents.contains_key(&handler) && !self.listeners.get(&handler).expect("invalid element id").contains::<Box<Fn(&mut UI, InputEvent)>>() {
        //         handler = *self.parents.get(&handler).expect("invalid element id");
        //     }
        //     self.fire(handler, ev);
        //     self.drain_queue();
        // }

        // if response.capture_keyboard {
        //     if let Some(ref focus) = self.keyboard_focus {
        //         focus.borrow_mut().handle_event(InputEvent::LostKeyboardFocus, self.input_state);
        //     }
        //     if let Some(ref responder) = response.responder {
        //         self.keyboard_focus = Some(responder.clone());
        //     }
        // }
        // if response.capture_mouse {
        //     if let Some(ref responder) = response.responder {
        //         self.mouse_focus = Some(responder.clone());
        //     }
        // }
        // if response.capture_mouse_position {
        //     self.mouse_position_captured = true;
        // }
        // if self.mouse_position_captured {
        //     if let Some(mouse_drag_origin) = self.input_state.mouse_drag_origin {
        //         ui_response.set_mouse_position = Some((mouse_drag_origin.x, mouse_drag_origin.y));
        //     }
        // }
        // ui_response.mouse_cursor = response.mouse_cursor;

        match ev {
            InputEvent::MouseRelease { button: MouseButton::Left } => {
                // if self.mouse_position_captured {
                //     if let Some(mouse_drag_origin) = self.input_state.mouse_drag_origin {
                //         self.input_state.mouse_position = mouse_drag_origin;
                //     }
                //     self.mouse_position_captured = false;
                // }

                self.input_state.mouse_drag_origin = None;
                // self.mouse_focus = None;
            }
            _ => {}
        }

        ui_response
    }

    fn find_element(&self, parent: usize, point: Point) -> usize {
        for child in self.children[parent].iter() {
            if self.layout[*child].contains_point(point) {
                return self.find_element(*child, point);
            }
        }
        parent
    }

    /* display */

    pub fn display(&self) -> DisplayList {
        let mut list = DisplayList::new();
        self.display_element(self.root, &mut list);
        list
    }

    fn display_element(&self, element: usize, list: &mut DisplayList) {
        self.elements[element].display(self, self.layout[element], list);
        for child in self.children[element].iter() {
            self.display_element(*child, list);
        }
    }

    /* layout */

    fn layout(&mut self) {
        let (_bounding_box, mut tree) = self.measure(self.root);
        let root = self.root;
        let bounds = BoundingBox::new(0.0, 0.0, self.width, self.height);
        self.arrange(root, bounds, &mut tree);
    }

    fn measure(&self, element: usize) -> (BoundingBox, BoundingBoxTree) {
        let mut child_boxes = Vec::new();
        let mut child_trees = Vec::new();
        if let Some(children) = self.children.get(element) {
            child_boxes.reserve(children.len());
            child_trees.reserve(children.len());
            for child in children {
                let (child_box, child_tree) = self.measure(*child);
                child_boxes.push(child_box);
                child_trees.push(child_tree);
            }
        }
        let bounding_box = self.elements[element].measure(self, &child_boxes[..]);
        (bounding_box, BoundingBoxTree { boxes: child_boxes, trees: child_trees })
    }

    fn arrange(&mut self, element: usize, bounds: BoundingBox, tree: &mut BoundingBoxTree) {
        let bounding_box = self.elements[element].arrange(self, bounds, &mut tree.boxes[..]);
        self.layout.insert(bounding_box);
        if let Some(children) = self.children.get(element).map(|children| children.clone()) {
            for (i, child) in children.iter().enumerate() {
                self.arrange(*child, tree.boxes[i], &mut tree.trees[i]);
            }
        }
    }
}

struct BoundingBoxTree {
    boxes: Vec<BoundingBox>,
    trees: Vec<BoundingBoxTree>,
}


pub struct Rectangle;

impl Element for Rectangle {
    type Model = [f32; 4];

    fn display(&self, model: &[f32; 4], bounds: BoundingBox, list: &mut DisplayList) {
        list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: *model });
    }
}


pub struct ContainerStyle {
    min_width: f32,
    max_width: f32,
    min_height: f32,
    max_height: f32,
}

impl Default for ContainerStyle {
    fn default() -> ContainerStyle {
        ContainerStyle {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        }
    }
}

pub struct Container;

impl Element for Container {
    type Model = ContainerStyle;

    fn measure(model: &ContainerStyle, children: &[BoundingBox]) -> BoundingBox {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;

        for child_box in children {
            width = width.max(child_box.size.x);
            height = height.max(child_box.size.y);
        }

        width = width.max(model.min_width).min(model.max_width);
        height = height.max(model.min_height).min(model.max_height);

        BoundingBox::new(0.0, 0.0, width, height)
    }

    fn arrange(model: &ContainerStyle, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
        for child_box in children.iter_mut() {
            *child_box = bounds;
        }

        bounds
    }
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


// style! {
//     struct TextStyle {
//         font: ResourceRef<Font<'static>>,
//         scale: Scale,
//     },
//     TextStylePatch
// }

// impl Default for TextStyle {
//     fn default() -> TextStyle {
//         TextStyle {
//             font: ResourceRef::new(0),
//             scale: Scale::uniform(14.0),
//         }
//     }
// }

// pub struct Label {
//     text: Cow<'static, str>,
//     glyphs: Vec<PositionedGlyph<'static>>,
//     size: Point,
// }

// impl Label {
//     pub fn with_text<S>(text: S) -> impl FnOnce(Context<Label>) -> Label where S: Into<Cow<'static, str>> {
//         move |mut ctx| {
//             let label = {
//                 let resources = ctx.resources();
//                 let box_style = resources.get_style::<BoxStyle>();
//                 let text_style = resources.get_style::<TextStyle>();
//                 let font = resources.get_resource::<Font>(text_style.font);

//                 let mut label = Label {
//                     text: text.into(),
//                     glyphs: Vec::new(),
//                     size: Point::new(0.0, 0.0),
//                 };
//                 label.size = label.layout(text_style.scale, font, Point::new(box_style.padding, box_style.padding));

//                 label
//             };

//             ctx.receive(|myself: &mut Label, ctx, s: String| {
//                 myself.text = s.into();
//             });
//             ctx.receive(|myself: &mut Label, ctx, s: &'static str| {
//                 myself.text = s.into();
//             });

//             label
//         }
//     }

//     fn layout(&mut self, scale: Scale, font: &Font, pos: Point) -> Point {
//         use unicode_normalization::UnicodeNormalization;
//         self.glyphs.clear();

//         let v_metrics = font.v_metrics(scale);
//         let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
//         let mut caret = point(pos.x, pos.y + v_metrics.ascent);
//         let mut last_glyph_id = None;
//         for c in self.text.nfc() {
//             if c.is_control() {
//                 match c {
//                     '\r' => {
//                         // caret = point(pos.x, caret.y + advance_height);
//                     }
//                     '\n' => {},
//                     _ => {}
//                 }
//                 continue;
//             }
//             let base_glyph = if let Some(glyph) = font.glyph(c) {
//                 glyph
//             } else {
//                 continue;
//             };
//             if let Some(id) = last_glyph_id.take() {
//                 caret.x += font.pair_kerning(scale, id, base_glyph.id());
//             }
//             last_glyph_id = Some(base_glyph.id());
//             let mut glyph = base_glyph.scaled(scale).positioned(caret);
//             // if let Some(bb) = glyph.pixel_bounding_box() {
//             //     if bb.max.x > (pos.x + size.x) as i32 {
//             //         caret = point(pos.x, caret.y + advance_height);
//             //         glyph = glyph.into_unpositioned().positioned(caret);
//             //         last_glyph_id = None;
//             //     }
//             // }
//             caret.x += glyph.unpositioned().h_metrics().advance_width;
//             self.glyphs.push(glyph.standalone());
//         }

//         Point::new(caret.x - pos.x, v_metrics.ascent - v_metrics.descent)
//     }
// }

// impl Element for Label {
//     fn measure(&self, resources: &Resources, children: &[BoundingBox]) -> BoundingBox {
//         let box_style = resources.get_style::<BoxStyle>();
//         BoundingBox { pos: Point::new(0.0, 0.0), size: self.size + Point::new(2.0 * box_style.padding, 2.0 * box_style.padding) }
//     }

//     fn arrange(&mut self, resources: &Resources, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
//         let box_style = resources.get_style::<BoxStyle>();
//         let text_style = resources.get_style::<TextStyle>();
//         let font = resources.get_resource::<Font>(text_style.font);
//         self.size = self.layout(text_style.scale, font, bounds.pos + Point::new(box_style.padding, box_style.padding));
//         BoundingBox { pos: bounds.pos, size: self.size + Point::new(2.0 * box_style.padding, 2.0 * box_style.padding) }
//     }

//     fn display(&self, resources: &Resources, bounds: BoundingBox, input_state: InputState, list: &mut DisplayList) {
//         let box_style = resources.get_style::<BoxStyle>();

//         list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: box_style.color });
//         for glyph in self.glyphs.iter() {
//             list.glyph(glyph.standalone());
//         }
//     }
// }


// pub struct Button;

// impl Button {
//     pub fn install(mut ctx: Context<Button>) -> Button {
//         let id = ctx.get_self();
//         ctx.register_slot(id);

//         ctx.listen(id, Button::handle);

//         Button
//     }

//     fn handle(&mut self, mut ctx: Context<Button>, evt: InputEvent) {
//         if let InputEvent::MouseRelease { button: MouseButton::Left } = evt {
//             ctx.fire(ClickEvent);
//         }
//     }
// }

// #[derive(Copy, Clone)]
// pub struct ClickEvent;

// impl Element for Button {
//     fn display(&self, resources: &Resources, bounds: BoundingBox, input_state: InputState, list: &mut DisplayList) {
//         let box_style = resources.get_style::<BoxStyle>();

//         let mut color = [0.15, 0.18, 0.23, 1.0];
//         if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
//             if bounds.contains_point(mouse_drag_origin) {
//                 color = [0.02, 0.2, 0.6, 1.0];
//             }
//         } else if bounds.contains_point(input_state.mouse_position) {
//             color = [0.3, 0.4, 0.5, 1.0];
//         }

//         list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: color });
//     }
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


#[derive(Copy, Clone, Debug)]
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
    pub mouse_position: Point,
    pub mouse_drag_origin: Option<Point>,
    pub mouse_left_pressed: bool,
    pub mouse_middle_pressed: bool,
    pub mouse_right_pressed: bool,
    pub modifiers: KeyboardModifiers,
}

impl Default for InputState {
    fn default() -> InputState {
        InputState {
            mouse_position: Point { x: -1.0, y: -1.0 },
            mouse_drag_origin: None,
            mouse_left_pressed: false,
            mouse_middle_pressed: false,
            mouse_right_pressed: false,
            modifiers: KeyboardModifiers::default(),
        }
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
