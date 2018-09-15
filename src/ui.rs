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

use std::rc::Rc;

use slab::Slab;

use glium::glutin;
use rusttype::{FontCollection, Font, Scale, point, PositionedGlyph};

use render::*;

/* references */

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ElementReference(usize);

macro_rules! reference {
    ($type:ident) => {
        pub struct $type<A> {
            index: usize,
            phantom_data: PhantomData<*const A>,
        }

        impl<A> Clone for $type<A> {
            fn clone(&self) -> $type<A> {
                $type {
                    index: self.index,
                    phantom_data: PhantomData,
                }
            }
        }

        impl<A> Copy for $type<A> {}

        impl<A> $type<A> {
            fn new(index: usize) -> $type<A> {
                $type {
                    index: index,
                    phantom_data: PhantomData,
                }
            }
        }
    }
}

reference! { Property }

impl<A> Property<A> {
    pub fn reference(&self) -> Reference<A> {
        Reference::new(self.index)
    }
}

reference! { Reference }

/* element */

pub trait Element {
    fn layout<'a>(&self, context: &Context, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in children {
            let (child_width, child_height) = child.layout(max_width, max_height);
            width = width.max(child_width);
            height = height.max(child_height);
        }
        (width, height)
    }

    fn display<'a>(&self, context: &Context, bounds: BoundingBox, list: &mut DisplayList) {}
}

pub struct ChildDelegate<'a> {
    reference: ElementReference,
    element: &'a Element,
    properties: &'a Slab<Box<UnsafeAny>>,
    bounds: BoundingBox,
    children: Vec<ChildDelegate<'a>>,
}

impl<'a> ChildDelegate<'a> {
    pub fn layout(&mut self, max_width: f32, max_height: f32) -> (f32, f32) {
        let (width, height) = self.element.layout(&Context { properties: self.properties }, max_width, max_height, &mut self.children);
        self.bounds.size.x = width;
        self.bounds.size.y = height;
        (width, height)
    }

    pub fn offset(&mut self, x: f32, y: f32) {
        self.bounds.pos.x = x;
        self.bounds.pos.y = y;
    }
}

pub struct Context<'a> {
    properties: &'a Slab<Box<UnsafeAny>>,
}

impl<'a> Context<'a> {
    pub fn get<A: 'static>(&self, reference: Reference<A>) -> &'a A {
        &*unsafe { self.properties[reference.index].downcast_ref_unchecked() }
    }
}

pub struct ContextMut<'a> {
    properties: &'a mut Slab<Box<UnsafeAny>>,
}

impl<'a> ContextMut<'a> {
    pub fn get<'b, A: 'static>(&'b self, property: Property<A>) -> &'b A {
        &*unsafe { self.properties[property.index].downcast_ref_unchecked() }
    }

    pub fn get_mut<'b, A: 'static>(&'b mut self, property: Property<A>) -> &'b mut A {
        &mut *unsafe { self.properties[property.index].downcast_mut_unchecked() }
    }

    pub fn set<A: 'static>(&mut self, property: Property<A>, value: A) {
        self.properties[property.index] = Box::new(value);
    }
}

/* ui */

pub struct UI {
    width: f32,
    height: f32,

    properties: Slab<Box<UnsafeAny>>,

    root: ElementReference,
    elements: Slab<Box<Element>>,
    parents: Slab<Option<ElementReference>>,
    children: Slab<Vec<ElementReference>>,
    layout: Slab<BoundingBox>,
    listeners: Slab<Option<Box<Fn(&mut ContextMut, ElementEvent)>>>,

    under_cursor: Vec<ElementReference>,

    input_state: InputState,
}

impl UI {
    pub fn new(width: f32, height: f32) -> UI {
        let mut ui = UI {
            width: width,
            height: height,

            properties: Slab::new(),

            root: ElementReference(0),
            elements: Slab::new(),
            parents: Slab::new(),
            children: Slab::new(),
            layout: Slab::new(),
            listeners: Slab::new(),

            under_cursor: Vec::new(),

            input_state: InputState::default(),
        };

        let root = ui.element(Empty, &[]);
        ui.root(root);

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

    pub fn root(&mut self, element: ElementReference) {
        self.root = element;
        self.layout();
    }

    pub fn element<E: Element + 'static>(&mut self, element: E, children: &[ElementReference]) -> ElementReference {
        let element = ElementReference(self.elements.insert(Box::new(element)));
        self.parents.insert(None);
        let mut children_vec = Vec::new();
        children_vec.extend_from_slice(children);
        self.children.insert(children_vec);
        for child in children {
            self.parents[child.0] = Some(element);
        }
        self.layout.insert(BoundingBox::new(0.0, 0.0, 0.0, 0.0));
        self.listeners.insert(None);
        element
    }

    pub fn element_with_listener<E: Element + 'static, F: Fn(&mut ContextMut, ElementEvent) + 'static>(&mut self, element: E, children: &[ElementReference], listener: F) -> ElementReference {
        let element = self.element(element, children);
        self.listeners[element.0] = Some(Box::new(listener));
        element
    }

    pub fn property<A: 'static>(&mut self, value: A) -> Property<A> {
        let index = self.properties.insert(Box::new(value));
        Property::new(index)
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
            InputEvent::MouseRelease(button) => {
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

        let mut ui_response: UIEventResponse = Default::default();

        let mut handler = match event {
            InputEvent::MouseMove(..) | InputEvent::MousePress(..) | InputEvent::MouseRelease(..) | InputEvent::MouseScroll(..) => {
                // if let Some(dragging) = self.dragging {
                //     Some(dragging)
                // } else {
                    let position = self.input_state.mouse_drag_origin.unwrap_or(self.input_state.mouse_position);
                    let mut i = 0;
                    let handler = if self.layout[self.root.0].contains_point(position) {
                        let mut element = self.root;
                        loop {
                            if i < self.under_cursor.len() {
                                if element != self.under_cursor[i] {
                                    let mut old_under_cursor = self.under_cursor.split_off(i);
                                    for child in old_under_cursor {
                                        self.fire_event(child, ElementEvent::MouseLeave);
                                    }
                                }
                            }
                            if i >= self.under_cursor.len() {
                                self.under_cursor.push(element);
                                self.fire_event(element, ElementEvent::MouseEnter);
                            }
                            i += 1;

                            let mut found = false;
                            for child in self.children[element.0].iter() {
                                if self.layout[child.0].contains_point(position) {
                                    element = *child;
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                break;
                            }
                        }

                        Some(element)
                    } else {
                        None
                    };

                    let mut old_under_cursor = self.under_cursor.split_off(i);
                    for child in old_under_cursor {
                        self.fire_event(child, ElementEvent::MouseLeave);
                    }

                    handler
                // }
            },
            InputEvent::KeyPress(..) | InputEvent::KeyRelease(..) | InputEvent::TextInput(..) => {
                // self.focus.or(Some(self.root))
                Some(self.root)
            }
            _ => {
                Some(self.root)
            }
        };

        if let Some(handler) = handler {
            self.bubble_event(handler, ElementEvent::from_input_event(event));
        }

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

        match event {
            InputEvent::MouseRelease(MouseButton::Left) => {
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

        self.layout();

        ui_response
    }

    fn bubble_event(&mut self, element: ElementReference, event: ElementEvent) {
        let mut handler = Some(element);
        while let Some(element) = handler {
            if self.fire_event(element, event) {
                break;
            } else {
                handler = self.parents[element.0];
            }
        }
    }

    fn fire_event(&mut self, element: ElementReference, event: ElementEvent) -> bool {
        if let Some(ref callback) = self.listeners[element.0] {
            callback(&mut ContextMut { properties: &mut self.properties }, event);
            true
        } else {
            false
        }
    }

    /* display */

    pub fn display(&self) -> DisplayList {
        let mut list = DisplayList::new();
        self.display_element(self.root, &mut list);
        list
    }

    fn display_element(&self, element: ElementReference, list: &mut DisplayList) {
        self.elements[element.0].display(&Context { properties: &self.properties }, self.layout[element.0], list);
        for child in self.children[element.0].iter() {
            self.display_element(*child, list);
        }
    }

    /* layout */

    fn layout(&mut self) {
        let mut root = Self::child_delegate(&self.properties, &self.children, &self.elements, self.root);
        let (width, height) = root.layout(self.width, self.height);
        Self::commit_delegates(&mut self.layout, root, Point::new(0.0, 0.0));
    }

    fn child_delegate<'a>(properties: &'a Slab<Box<UnsafeAny>>, children: &'a Slab<Vec<ElementReference>>, elements: &'a Slab<Box<Element>>, reference: ElementReference) -> ChildDelegate<'a> {
        let mut child_delegates: Vec<ChildDelegate<'a>> = Vec::new();
        let children_indices = &children[reference.0];
        child_delegates.reserve(children_indices.len());
        for child in children_indices {
            child_delegates.push(Self::child_delegate(properties, children, elements, *child));
        }
        ChildDelegate {
            reference: reference,
            element: &*elements[reference.0],
            properties: properties,
            bounds: BoundingBox::new(0.0, 0.0, 0.0, 0.0),
            children: child_delegates,
        }
    }

    fn commit_delegates(layout: &mut Slab<BoundingBox>, delegate: ChildDelegate, offset: Point) {
        let offset = offset + delegate.bounds.pos;
        layout[delegate.reference.0].pos = offset;
        layout[delegate.reference.0].size = delegate.bounds.size;
        for child in delegate.children {
            Self::commit_delegates(layout, child, offset);
        }
    }
}


pub struct Empty;

impl Element for Empty {}


pub struct BackgroundColor {
    pub color: Reference<[f32; 4]>,
}

impl Element for BackgroundColor {
    fn display(&self, ctx: &Context, bounds: BoundingBox, list: &mut DisplayList) {
        list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: *ctx.get(self.color) });
    }
}


pub struct Container {
    max_size: Reference<(f32, f32)>,
}

impl Element for Container {
    fn layout(&self, ctx: &Context, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let (self_max_width, self_max_height) = *ctx.get(self.max_size);
        let max_width = self_max_width.min(max_width);
        let max_height = self_max_height.min(max_height);

        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;

        for child in children {
            let (child_width, child_height) = child.layout(max_width, max_height);
            width = width.max(child_width);
            height = height.max(child_height);
        }
        (width, height)
    }
}


pub struct Padding {
    pub padding: Reference<f32>,
}

impl Element for Padding {
    fn layout(&self, ctx: &Context, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let padding = *ctx.get(self.padding);
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in children {
            let (child_width, child_height) = child.layout(max_width - 2.0 * padding, max_height - 2.0 * padding);
            width = width.max(child_width);
            height = height.max(child_height);
            child.offset(padding, padding);
        }
        (width + 2.0 * padding, height + 2.0 * padding)
    }
}


pub struct Row {
    pub spacing: Reference<f32>,
}

impl Element for Row {
    fn layout(&self, ctx: &Context, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let spacing = *ctx.get(self.spacing);
        let mut x: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in children {
            let (child_width, child_height) = child.layout(max_width - x, max_height);
            child.offset(x, 0.0);
            x += child_width + spacing;
            height = height.max(child_height);
        }
        (x - spacing, height)
    }
}


pub struct Column {
    pub spacing: f32,
}

impl Element for Column {
    fn layout(&self, ctx: &Context, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
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
    pub font: Rc<Font<'static>>,
    pub scale: Scale,
}

pub struct Text {
    pub text: String,
    pub style: TextStyle,
}

impl Text {
    fn layout_text(&self, x: f32, y: f32, max_width: f32, max_height: f32) -> (Vec<PositionedGlyph<'static>>, (f32, f32)) {
        use unicode_normalization::UnicodeNormalization;

        let mut glyphs = Vec::new();
        let mut wrapped = false;

        let v_metrics = self.style.font.v_metrics(self.style.scale);
        let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let mut caret = point(x, y + v_metrics.ascent);
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
                if bb.max.x > (x + max_width) as i32 {
                    wrapped = true;
                    caret = point(x, caret.y + advance_height);
                    glyph = glyph.into_unpositioned().positioned(caret);
                    last_glyph_id = None;
                }
            }
            caret.x += glyph.unpositioned().h_metrics().advance_width;
            glyphs.push(glyph.standalone());
        }

        let width = if wrapped { max_width } else { caret.x };
        (glyphs, (width, v_metrics.ascent - v_metrics.descent))
    }
}

impl Element for Text {
    fn layout(&self, ctx: &Context, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let (_, (width, height)) = self.layout_text(0.0, 0.0, max_width, max_height);
        (width, height)
    }

    fn display(&self, ctx: &Context, bounds: BoundingBox, list: &mut DisplayList) {
        let (glyphs, _) = self.layout_text(bounds.pos.x, bounds.pos.y, bounds.size.x, bounds.size.y);
        for glyph in glyphs.iter() {
            list.glyph(glyph.standalone());
        }
    }
}


// //         let mut color = [0.15, 0.18, 0.23, 1.0];
// //         if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
// //             if bounds.contains_point(mouse_drag_origin) {
// //                 color = [0.02, 0.2, 0.6, 1.0];
// //             }
// //         } else if bounds.contains_point(input_state.mouse_position) {
// //             color = [0.3, 0.4, 0.5, 1.0];
// //         }

// pub struct Button;

// impl Button {
//     pub fn new(child: Tree) -> Component<Button> {
//         component(Button { hover: false }, |cmp| {
//             element(cmp.map(|button| {
//                 BackgroundColor {
//                     color: if button.hover { [0.3, 0.4, 0.5, 1.0] } else { [0.15, 0.18, 0.23, 1.0] }
//                 }
//             })).on(cmp, |cmp, ev, ctx| {
//                 if let InputEvent::MousePress { button: MouseButton::Left } = ev {
//                     cmp.hover = !cmp.hover;
//                 }
//             }).children(vec![child]).into()
//         })
//     }
// }


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
    MouseMove(Point),
    MousePress(MouseButton),
    MouseRelease(MouseButton),
    MouseScroll(f32),
    KeyPress(KeyboardButton),
    KeyRelease(KeyboardButton),
    TextInput(char),
}

#[derive(Copy, Clone, Debug)]
pub enum ElementEvent {
    MouseEnter,
    MouseLeave,
    MouseMove(Point),
    MousePress(MouseButton),
    MouseRelease(MouseButton),
    MouseScroll(f32),
    KeyPress(KeyboardButton),
    KeyRelease(KeyboardButton),
    TextInput(char),
}

impl ElementEvent {
    fn from_input_event(event: InputEvent) -> ElementEvent {
        match event {
            InputEvent::MouseMove(Point) => ElementEvent::MouseMove(Point),
            InputEvent::MousePress(MouseButton) => ElementEvent::MousePress(MouseButton),
            InputEvent::MouseRelease(MouseButton) => ElementEvent::MouseRelease(MouseButton),
            InputEvent::MouseScroll(f32) => ElementEvent::MouseScroll(f32),
            InputEvent::KeyPress(KeyboardButton) => ElementEvent::KeyPress(KeyboardButton),
            InputEvent::KeyRelease(KeyboardButton) => ElementEvent::KeyRelease(KeyboardButton),
            InputEvent::TextInput(char) => ElementEvent::TextInput(char),
        }
    }
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
