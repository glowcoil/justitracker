use unsafe_any::UnsafeAny;

use std::marker::PhantomData;
use std::f32;

use std::rc::Rc;
use std::cell::RefCell;

use slab::Slab;
use priority_queue::PriorityQueue;
use std::cmp::Reverse;
use anymap::AnyMap;

use glium::glutin;
use rusttype::{Font, Scale, point, PositionedGlyph};

use render::*;

/* references */

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ElementRef {
    index: usize,
    component: Option<usize>,
}

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

reference! { Prop }
reference! { Ref }
reference! { ComponentRef }

impl<A> From<Prop<A>> for Ref<A> {
    fn from(prop: Prop<A>) -> Ref<A> {
        Ref::new(prop.index)
    }
}

pub enum RefOrValue<A> {
    Ref(Ref<A>),
    Value(A),
}

impl<A: 'static> RefOrValue<A> {
    fn get<'a, C: GetProperty>(&'a self, context: &'a C) -> &'a A {
        match *self {
            RefOrValue::Ref(reference) => context.get(reference),
            RefOrValue::Value(ref value) => value,
        }
    }
}

impl<A> From<Prop<A>> for RefOrValue<A> {
    fn from(prop: Prop<A>) -> RefOrValue<A> {
        RefOrValue::Ref(Ref::new(prop.index))
    }
}

impl<A> From<Ref<A>> for RefOrValue<A> {
    fn from(reference: Ref<A>) -> RefOrValue<A> {
        RefOrValue::Ref(reference)
    }
}

impl<A> From<A> for RefOrValue<A> {
    fn from(value: A) -> RefOrValue<A> {
        RefOrValue::Value(value)
    }
}

/* element */

pub trait Element {
    fn layout(&self, _context: &ElementContext, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for child in children {
            let (child_width, child_height) = child.layout(max_width, max_height);
            width = width.max(child_width);
            height = height.max(child_height);
        }
        (width, height)
    }

    fn display(&self, _context: &ElementContext, _bounds: BoundingBox, _list: &mut DisplayList) {}
}

pub struct ChildDelegate<'a> {
    reference: usize,
    element: &'a Element,
    context: &'a Context,
    bounds: BoundingBox,
    children: Vec<ChildDelegate<'a>>,
}

impl<'a> ChildDelegate<'a> {
    pub fn layout(&mut self, max_width: f32, max_height: f32) -> (f32, f32) {
        let (width, height) = self.element.layout(&ElementContext(self.context), max_width, max_height, &mut self.children);
        self.bounds.size.x = width;
        self.bounds.size.y = height;
        (width, height)
    }

    pub fn offset(&mut self, x: f32, y: f32) {
        self.bounds.pos.x = x;
        self.bounds.pos.y = y;
    }
}

pub struct Context {
    properties: Slab<Box<UnsafeAny>>,
    dependencies: Slab<Vec<usize>>,
    dependents: Slab<Vec<usize>>,
    update: Slab<Option<Box<Fn(&mut Slab<Box<UnsafeAny>>)>>>,
    priorities: Slab<u32>,
    queue: PriorityQueue<usize, Reverse<u32>>,
}

impl Context {
    fn new() -> Context {
        Context {
            properties: Slab::new(),
            dependencies: Slab::new(),
            dependents: Slab::new(),
            update: Slab::new(),
            priorities: Slab::new(),
            queue: PriorityQueue::new(),
        }
    }

    pub fn get<'a, A: 'static, R: Into<Ref<A>>>(&'a self, reference: R) -> &'a A {
        Context::get_prop(&self.properties, reference.into())
    }

    pub fn get_mut<'a, A: 'static>(&'a mut self, prop: Prop<A>) -> &'a mut A {
        self.queue.push(prop.index, Reverse(self.priorities[prop.index]));
        &mut *unsafe { self.properties[prop.index].downcast_mut_unchecked() }
    }

    pub fn set<A: 'static>(&mut self, prop: Prop<A>, value: A) {
        self.queue.push(prop.index, Reverse(self.priorities[prop.index]));
        self.properties[prop.index] = Box::new(value);
    }

    fn get_prop<'a, A: 'static>(props: &'a Slab<Box<UnsafeAny>>, reference: Ref<A>) -> &'a A {
        &*unsafe { props[reference.index].downcast_ref_unchecked() }
    }
}

pub struct ElementContext<'a>(&'a Context);

impl<'a> ElementContext<'a> {
    pub fn get<'b, A: 'static, R: Into<Ref<A>>>(&'b self, reference: R) -> &'b A {
        self.0.get(reference)
    }
}

pub struct ComponentContext<'a, C> {
    component: ComponentRef<C>,
    context: &'a mut Context,
    input_state: &'a mut InputState,
    response: UIEventResponse,
}

impl<'a, C> ComponentContext<'a, C> {
    fn new<'b>(component: ComponentRef<C>, context: &'b mut Context, input_state: &'b mut InputState) -> ComponentContext<'b, C> {
        ComponentContext {
            component: component,
            context: context,
            input_state: input_state,
            response: UIEventResponse::default(),
        }
    }

    pub fn get<'b, A: 'static, R: Into<Ref<A>>>(&'b self, reference: R) -> &'b A {
        self.context.get(reference)
    }

    pub fn get_mut<'b, A: 'static>(&'b mut self, prop: Prop<A>) -> &'b mut A {
        self.context.get_mut(prop)
    }

    pub fn set<A: 'static>(&mut self, prop: Prop<A>, value: A) {
        self.context.set(prop, value)
    }

    pub fn focus(&mut self, element: ElementRef) {
        self.input_state.focus = Some(element.index);
    }

    pub fn fire<E: 'static>(&mut self, event: E) {

    }

    pub fn defocus(&mut self, element: ElementRef) {
        if self.input_state.focus == Some(element.index) {
            self.input_state.focus = None;
        }
    }

    pub fn capture_mouse(&mut self, element: ElementRef) {
        self.input_state.mouse_focus = Some(element.index);
    }

    pub fn relinquish_mouse(&mut self, element: ElementRef) {
        if self.input_state.mouse_focus == Some(element.index) {
            self.input_state.mouse_focus = None;
        }
    }

    pub fn get_mouse_position(&mut self) -> Point {
        self.input_state.mouse_position
    }

    pub fn set_mouse_position(&mut self, position: Point) {
        self.response.mouse_position = Some(position);
    }

    pub fn set_cursor(&mut self, cursor: MouseCursor) {
        self.response.mouse_cursor = Some(cursor);
    }

    pub fn hide_cursor(&mut self) {
        self.response.hide_cursor = Some(true);
    }

    pub fn show_cursor(&mut self) {
        self.response.hide_cursor = Some(false);
    }
}

pub struct TreeContext<'a>(&'a mut UI);

impl<'a> TreeContext<'a> {
    pub fn get<'b, A: 'static, R: Into<Ref<A>>>(&'b self, reference: R) -> &'b A {
        self.0.context.get(reference)
    }
}

pub trait GetProperty {
    fn get<'a, A: 'static>(&'a self, reference: Ref<A>) -> &'a A;
}

impl GetProperty for Context {
    fn get<'a, A: 'static>(&'a self, reference: Ref<A>) -> &'a A {
        self.get(reference)
    }
}

impl<'b> GetProperty for ElementContext<'b> {
    fn get<'a, A: 'static>(&'a self, reference: Ref<A>) -> &'a A {
        self.get(reference)
    }
}

impl<'b, C> GetProperty for ComponentContext<'b, C> {
    fn get<'a, A: 'static>(&'a self, reference: Ref<A>) -> &'a A {
        self.get(reference)
    }
}

impl<'b> GetProperty for TreeContext<'b> {
    fn get<'a, A: 'static>(&'a self, reference: Ref<A>) -> &'a A {
        self.get(reference)
    }
}

pub trait Install {
    fn element<E: Element + 'static>(&mut self, element: E, children: &[ElementRef]) -> ElementRef;
    fn tree<F: Fn(&mut TreeContext) -> ElementRef + 'static>(&mut self, f: F) -> ElementRef;
    fn listen<E: 'static, C: 'static>(&mut self, element: ElementRef, listener: ComponentRef<C>, callback: fn(&mut C, &mut ComponentContext<C>, E));
    fn component<C: 'static>(&mut self, component: C) -> ComponentRef<C>;
    fn bind<C>(&mut self, component: ComponentRef<C>, element: ElementRef) -> ElementRef;
    fn prop<A: 'static>(&mut self, value: A) -> Prop<A>;
    fn map<A: 'static, B: 'static, F>(&mut self, a: impl Into<Ref<A>>, f: F) -> Ref<B> where F: Fn(&A) -> B + 'static;
}

impl Install for UI {
    fn element<E: Element + 'static>(&mut self, element: E, children: &[ElementRef]) -> ElementRef {
        self.element(element, children)
    }
    fn tree<F: Fn(&mut TreeContext) -> ElementRef + 'static>(&mut self, f: F) -> ElementRef {
        self.tree(f)
    }
    fn listen<E: 'static, C: 'static>(&mut self, element: ElementRef, listener: ComponentRef<C>, callback: fn(&mut C, &mut ComponentContext<C>, E)) {
        self.listen(element, listener, callback);
    }
    fn component<C: 'static>(&mut self, component: C) -> ComponentRef<C> {
        self.component(component)
    }
    fn bind<C>(&mut self, component: ComponentRef<C>, element: ElementRef) -> ElementRef {
        self.bind(component, element)
    }
    fn prop<A: 'static>(&mut self, value: A) -> Prop<A> {
        self.prop(value)
    }
    fn map<A: 'static, B: 'static, F>(&mut self, a: impl Into<Ref<A>>, f: F) -> Ref<B> where F: Fn(&A) -> B + 'static {
        self.map(a, f)
    }
}

impl<'b> Install for TreeContext<'b> {
    fn element<E: Element + 'static>(&mut self, element: E, children: &[ElementRef]) -> ElementRef {
        self.0.element(element, children)
    }
    fn tree<F: Fn(&mut TreeContext) -> ElementRef + 'static>(&mut self, f: F) -> ElementRef {
        self.0.tree(f)
    }
    fn listen<E: 'static, C: 'static>(&mut self, element: ElementRef, listener: ComponentRef<C>, callback: fn(&mut C, &mut ComponentContext<C>, E)) {
        self.0.listen(element, listener, callback);
    }
    fn component<C: 'static>(&mut self, component: C) -> ComponentRef<C> {
        self.0.component(component)
    }
    fn bind<C>(&mut self, component: ComponentRef<C>, element: ElementRef) -> ElementRef {
        self.0.bind(component, element)
    }
    fn prop<A: 'static>(&mut self, value: A) -> Prop<A> {
        self.0.prop(value)
    }
    fn map<A: 'static, B: 'static, F>(&mut self, a: impl Into<Ref<A>>, f: F) -> Ref<B> where F: Fn(&A) -> B + 'static {
        self.0.map(a, f)
    }
}

/* ui */

pub struct UI {
    width: f32,
    height: f32,

    context: Context,

    root: usize,
    elements: Slab<Box<Element>>,
    parents: Slab<Option<usize>>,
    children: Slab<Vec<usize>>,
    layout: Slab<BoundingBox>,
    components: Slab<Box<UnsafeAny>>,

    element_listeners: Slab<AnyMap>,
    component_listeners: Slab<AnyMap>,

    dynamics: Slab<Rc<Fn(&mut TreeContext) -> ElementRef>>,
    dynamic_indices: Slab<usize>,

    under_cursor: Vec<usize>,
    input_state: InputState,
}

impl UI {
    pub fn new(width: f32, height: f32) -> UI {
        let mut ui = UI {
            width: width,
            height: height,

            context: Context::new(),

            root: 0,
            elements: Slab::new(),
            parents: Slab::new(),
            children: Slab::new(),
            layout: Slab::new(),
            components: Slab::new(),

            element_listeners: Slab::new(),
            component_listeners: Slab::new(),

            dynamics: Slab::new(),
            dynamic_indices: Slab::new(),

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

    pub fn root(&mut self, element: ElementRef) {
        self.root = element.index;
        self.layout();
    }

    pub fn element<E: Element + 'static>(&mut self, element: E, children: &[ElementRef]) -> ElementRef {
        let index = self.elements.insert(Box::new(element));
        self.parents.insert(None);
        let mut children_vec = Vec::new();
        for child in children {
            children_vec.push(child.index);
            self.parents[child.index] = Some(index);
        }
        self.children.insert(children_vec);
        self.layout.insert(BoundingBox::new(0.0, 0.0, 0.0, 0.0));
        self.element_listeners.insert(AnyMap::new());
        ElementRef { index: index, component: None }
    }

    pub fn tree<F: Fn(&mut TreeContext) -> ElementRef + 'static>(&mut self, f: F) -> ElementRef {
        let element = self.element(Empty, &[]);
        self.dynamics.insert(Rc::new(f));
        self.dynamic_indices.insert(element.index);
        element
    }

    pub fn listen<E: 'static, C: 'static>(&mut self, element: ElementRef, listener: ComponentRef<C>, callback: fn(&mut C, &mut ComponentContext<C>, E)) {
        let callback = Box::new(move |component: &mut UnsafeAny, context: &mut Context, input_state: &mut InputState, event: E| {
            let mut component_context = ComponentContext::new(listener, context, input_state);
            callback(unsafe { component.downcast_mut_unchecked() }, &mut component_context, event);
            component_context.response
        });
        if let Some(component) = element.component {
            self.component_listeners[component].insert::<(usize, Box<Fn(&mut UnsafeAny, &mut Context, &mut InputState, E) -> UIEventResponse>)>((listener.index, callback));
        } else {
            self.element_listeners[element.index].insert::<(usize, Box<Fn(&mut UnsafeAny, &mut Context, &mut InputState, E) -> UIEventResponse>)>((listener.index, callback));
        }
    }

    pub fn component<C: 'static>(&mut self, component: C) -> ComponentRef<C> {
        let index = self.components.insert(Box::new(component));
        self.component_listeners.insert(AnyMap::new());
        ComponentRef::new(index)
    }

    pub fn bind<C>(&mut self, component: ComponentRef<C>, element: ElementRef) -> ElementRef {
        ElementRef { index: element.index, component: Some(component.index) }
    }

    pub fn prop<A: 'static>(&mut self, value: A) -> Prop<A> {
        let index = self.context.properties.insert(Box::new(value));
        self.context.dependencies.insert(Vec::new());
        self.context.dependents.insert(Vec::new());
        self.context.update.insert(None);
        self.context.priorities.insert(0);
        Prop::new(index)
    }

    pub fn map<A: 'static, B: 'static, F>(&mut self, a: impl Into<Ref<A>>, f: F) -> Ref<B> where F: Fn(&A) -> B + 'static {
        let a = a.into();
        let value = f(self.context.get(a));
        let b = self.prop(value);
        self.context.dependencies[b.index].push(a.index);
        self.context.dependents[a.index].push(b.index);
        self.context.priorities[b.index] = self.context.priorities[a.index] + 1;
        self.context.update[b.index] = Some(Box::new(move |props| {
            let value = f(Context::get_prop(props, a));
            props[b.index] = Box::new(value);
        }));
        b.into()
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

        let mut ui_response: UIEventResponse = Default::default();

        let handler = match event {
            InputEvent::MouseMove(..) | InputEvent::MousePress(..) | InputEvent::MouseRelease(..) | InputEvent::MouseScroll(..) => {
                if self.input_state.mouse_focus.is_some() {
                    self.input_state.mouse_focus
                } else {
                    let mut i = 0;
                    let handler = if self.layout[self.root].contains_point(self.input_state.mouse_position) {
                        let mut element = self.root;
                        loop {
                            if i < self.under_cursor.len() {
                                if element != self.under_cursor[i] {
                                    let mut old_under_cursor = self.under_cursor.split_off(i);
                                    for child in old_under_cursor {
                                        if let Some(response) = self.fire_event::<ElementEvent>(child, ElementEvent::MouseLeave) {
                                            ui_response.merge(response);
                                        }
                                    }
                                }
                            }
                            if i >= self.under_cursor.len() {
                                self.under_cursor.push(element);
                                if let Some(response) = self.fire_event::<ElementEvent>(element, ElementEvent::MouseEnter) {
                                    ui_response.merge(response);
                                }
                            }
                            i += 1;

                            let mut found = false;
                            for child in self.children[element].iter() {
                                if self.layout[*child].contains_point(self.input_state.mouse_position) {
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
                        if let Some(response) = self.fire_event::<ElementEvent>(child, ElementEvent::MouseLeave) {
                            ui_response.merge(response);
                        }
                    }

                    handler
                }
            },
            InputEvent::KeyPress(..) | InputEvent::KeyRelease(..) | InputEvent::TextInput(..) => {
                self.input_state.focus.or(Some(self.root))
            }
        };

        if let Some(handler) = handler {
            if let Some(response) = self.bubble_event(handler, ElementEvent::from_input_event(event)) {
                ui_response.merge(response);
            }
        }

        ui_response
    }

    fn bubble_event(&mut self, element: usize, event: ElementEvent) -> Option<UIEventResponse> {
        let mut handler = Some(element);
        let mut response = None;
        while let Some(element) = handler {
            response = self.fire_event::<ElementEvent>(element, event);
            if response.is_some() {
                break;
            } else {
                handler = self.parents[element];
            }
        }
        response
    }

    fn fire_event<E: 'static>(&mut self, element: usize, event: E) -> Option<UIEventResponse> {
        if let Some((listener, ref callback)) = self.element_listeners[element].get::<(usize, Box<Fn(&mut UnsafeAny, &mut Context, &mut InputState, E) -> UIEventResponse>)>() {
            Some(callback(&mut *self.components[*listener], &mut self.context, &mut self.input_state, event))
        } else {
            None
        }
    }

    pub fn update(&mut self) {
        while let Some((index, _)) = self.context.queue.pop() {
            if let Some(ref update) = self.context.update[index] {
                update(&mut self.context.properties);
            }
            for dependent in self.context.dependents[index].iter() {
                self.context.queue.push(*dependent, Reverse(self.context.priorities[*dependent]));
            }
        }
        for i in 0..self.dynamics.len() {
            let f = self.dynamics[i].clone();
            self.children[self.dynamic_indices[i]] = vec![f(&mut TreeContext(self)).index];
        }
        self.layout();
    }

    /* display */

    pub fn display(&self) -> DisplayList {
        let mut list = DisplayList::new();
        self.display_element(self.root, &mut list);
        list
    }

    fn display_element(&self, element: usize, list: &mut DisplayList) {
        self.elements[element].display(&ElementContext(&self.context), self.layout[element], list);
        for child in self.children[element].iter() {
            self.display_element(*child, list);
        }
    }

    /* layout */

    fn layout(&mut self) {
        let mut root = Self::child_delegate(&self.context, &self.children, &self.elements, self.root);
        root.layout(self.width, self.height);
        Self::commit_delegates(&mut self.layout, root, Point::new(0.0, 0.0));
    }

    fn child_delegate<'a>(context: &'a Context, children: &'a Slab<Vec<usize>>, elements: &'a Slab<Box<Element>>, reference: usize) -> ChildDelegate<'a> {
        let mut child_delegates: Vec<ChildDelegate<'a>> = Vec::new();
        let children_indices = &children[reference];
        child_delegates.reserve(children_indices.len());
        for child in children_indices {
            child_delegates.push(Self::child_delegate(context, children, elements, *child));
        }
        ChildDelegate {
            reference: reference,
            element: &*elements[reference],
            context: context,
            bounds: BoundingBox::new(0.0, 0.0, 0.0, 0.0),
            children: child_delegates,
        }
    }

    fn commit_delegates(layout: &mut Slab<BoundingBox>, delegate: ChildDelegate, offset: Point) {
        let offset = offset + delegate.bounds.pos;
        layout[delegate.reference].pos = offset;
        layout[delegate.reference].size = delegate.bounds.size;
        for child in delegate.children {
            Self::commit_delegates(layout, child, offset);
        }
    }
}

struct InputState {
    mouse_position: Point,
    mouse_left_pressed: bool,
    mouse_middle_pressed: bool,
    mouse_right_pressed: bool,
    modifiers: KeyboardModifiers,
    focus: Option<usize>,
    mouse_focus: Option<usize>,
}

impl Default for InputState {
    fn default() -> InputState {
        InputState {
            mouse_position: Point { x: -1.0, y: -1.0 },
            mouse_left_pressed: false,
            mouse_middle_pressed: false,
            mouse_right_pressed: false,
            modifiers: KeyboardModifiers::default(),
            focus: None,
            mouse_focus: None,
        }
    }
}


pub struct Empty;

impl Empty {
    pub fn new() -> Empty {
        Empty
    }

    pub fn install(self, ui: &mut impl Install) -> ElementRef {
        ui.element(self, &[])
    }
}

impl Element for Empty {}


pub struct BackgroundColor {
    color: RefOrValue<[f32; 4]>,
}

impl BackgroundColor {
    pub fn new(color: RefOrValue<[f32; 4]>) -> BackgroundColor {
        BackgroundColor { color: color }
    }

    pub fn install(self, ui: &mut impl Install, child: ElementRef) -> ElementRef {
        ui.element(self, &[child])
    }
}

impl Element for BackgroundColor {
    fn display(&self, ctx: &ElementContext, bounds: BoundingBox, list: &mut DisplayList) {
        list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: *self.color.get(ctx) });
    }
}


pub struct Container {
    max_size: RefOrValue<(f32, f32)>,
}

impl Container {
    pub fn new(max_size: RefOrValue<(f32, f32)>) -> Container {
        Container { max_size: max_size }
    }

    pub fn install(self, ui: &mut impl Install, child: ElementRef) -> ElementRef {
        ui.element(self, &[child])
    }
}

impl Element for Container {
    fn layout(&self, ctx: &ElementContext, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let (self_max_width, self_max_height) = *self.max_size.get(ctx);
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
    padding: RefOrValue<f32>,
}

impl Padding {
    pub fn new(padding: RefOrValue<f32>) -> Padding {
        Padding { padding: padding }
    }

    pub fn install(self, ui: &mut impl Install, child: ElementRef) -> ElementRef {
        ui.element(self, &[child])
    }
}

impl Element for Padding {
    fn layout(&self, ctx: &ElementContext, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let padding = *self.padding.get(ctx);
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
    spacing: RefOrValue<f32>,
}

impl Row {
    pub fn new(spacing: RefOrValue<f32>) -> Row {
        Row { spacing: spacing }
    }

    pub fn install(self, ui: &mut impl Install, children: &[ElementRef]) -> ElementRef {
        ui.element(self, children)
    }
}

impl Element for Row {
    fn layout(&self, ctx: &ElementContext, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let spacing = *self.spacing.get(ctx);
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


pub struct Col {
    spacing: RefOrValue<f32>,
}

impl Col {
    pub fn new(spacing: RefOrValue<f32>) -> Col {
        Col { spacing: spacing }
    }

    pub fn install(self, ui: &mut impl Install, children: &[ElementRef]) -> ElementRef {
        ui.element(self, children)
    }
}

impl Element for Col {
    fn layout(&self, ctx: &ElementContext, max_width: f32, max_height: f32, children: &mut [ChildDelegate]) -> (f32, f32) {
        let spacing = *self.spacing.get(ctx);
        let mut width: f32 = 0.0;
        let mut y: f32 = 0.0;
        for child in children {
            let (child_width, child_height) = child.layout(max_width, max_height - y);
            child.offset(0.0, y);
            width = width.max(child_width);
            y += child_height + spacing;
        }
        (width, y - spacing)
    }
}


pub struct TextStyle {
    pub font: Font<'static>,
    pub scale: Scale,
}

pub struct Text {
    text: RefOrValue<String>,
    style: Ref<TextStyle>,
    glyphs: RefCell<Vec<PositionedGlyph<'static>>>,
}

impl Text {
    pub fn new(text: RefOrValue<String>, style: Ref<TextStyle>) -> Text {
        Text { text: text, style: style, glyphs: RefCell::new(Vec::new()) }
    }

    pub fn install(self, ui: &mut impl Install) -> ElementRef {
        ui.element(self, &[])
    }

    fn layout_text(&self, text: &str, max_width: f32, _max_height: f32, font: &Font<'static>, scale: Scale) -> (f32, f32) {
        use unicode_normalization::UnicodeNormalization;

        let mut glyphs = self.glyphs.borrow_mut();
        glyphs.clear();
        let mut wrapped = false;

        let v_metrics = font.v_metrics(scale);
        let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let mut caret = point(0.0, v_metrics.ascent);
        let mut last_glyph_id = None;
        for c in text.nfc() {
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
        (width, v_metrics.ascent - v_metrics.descent)
    }
}

impl Element for Text {
    fn layout(&self, ctx: &ElementContext, max_width: f32, max_height: f32, _children: &mut [ChildDelegate]) -> (f32, f32) {
        let text = self.text.get(ctx);
        let style = ctx.get(self.style);

        self.layout_text(text, max_width, max_height, &style.font, style.scale)
    }

    fn display(&self, _ctx: &ElementContext, bounds: BoundingBox, list: &mut DisplayList) {
        for glyph in self.glyphs.borrow().iter() {
            let position = glyph.position();
            list.glyph(glyph.clone().into_unpositioned().positioned(point(bounds.pos.x + position.x, bounds.pos.y + position.y)));
        }
    }
}


#[derive(Copy, Clone, Eq, PartialEq)]
enum ButtonState {
    Up,
    Hover,
    Down,
}

pub struct Button {
    state: Prop<ButtonState>,
}

pub struct ClickEvent;

impl Button {
    pub fn with_text(ui: &mut impl Install, text: RefOrValue<String>, style: Ref<TextStyle>) -> ElementRef {
        let text = Text::new(text, style).install(ui);
        Button::install(ui, text)
    }

    pub fn install(ui: &mut impl Install, child: ElementRef) -> ElementRef {
        let state = ui.prop(ButtonState::Up);

        let button = ui.component(Button { state: state });

        let color = ui.map(state, |state| {
            match state {
                ButtonState::Up => [0.15, 0.18, 0.23, 1.0],
                ButtonState::Hover => [0.3, 0.4, 0.5, 1.0],
                ButtonState::Down => [0.02, 0.2, 0.6, 1.0],
            }
        });

        let padding = Padding::new(10.0f32.into()).install(ui, child);
        let background = BackgroundColor::new(color.into()).install(ui, padding);

        ui.listen(background, button, Self::handle);

        ui.bind(button, background)
    }

    fn handle(&mut self, ctx: &mut ComponentContext<Button>, event: ElementEvent) {
        match event {
            ElementEvent::MouseEnter => {
                ctx.set(self.state, ButtonState::Hover);
            }
            ElementEvent::MouseLeave => {
                ctx.set(self.state, ButtonState::Up);
            }
            ElementEvent::MousePress(MouseButton::Left) => {
                ctx.set(self.state, ButtonState::Down);
            }
            ElementEvent::MouseRelease(MouseButton::Left) => {
                if *ctx.get(self.state) == ButtonState::Down {
                    ctx.set(self.state, ButtonState::Hover);
                    ctx.fire::<ClickEvent>(ClickEvent);
                }
            }
            _ => {}
        }
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
            InputEvent::MouseMove(point) => ElementEvent::MouseMove(point),
            InputEvent::MousePress(button) => ElementEvent::MousePress(button),
            InputEvent::MouseRelease(button) => ElementEvent::MouseRelease(button),
            InputEvent::MouseScroll(delta) => ElementEvent::MouseScroll(delta),
            InputEvent::KeyPress(button) => ElementEvent::KeyPress(button),
            InputEvent::KeyRelease(button) => ElementEvent::KeyRelease(button),
            InputEvent::TextInput(character) => ElementEvent::TextInput(character),
        }
    }
}

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
