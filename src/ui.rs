use std::collections::{HashMap, HashSet};

use anymap::AnyMap;
use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::mem;
use std::borrow::BorrowMut;
use std::borrow::Cow;

use std::f32;

use glium::glutin;
use rusttype::{Font, Scale, point, PositionedGlyph};

use render::*;

pub type ElementRef = usize;

type ClassRef = usize;

#[derive(Copy, Clone)]
pub struct ResourceRef<R> {
    index: Option<usize>,
    resource_type: PhantomData<R>,
}

impl<R: 'static> ResourceRef<R> {
    fn new(index: usize) -> ResourceRef<R> { ResourceRef { index: Some(index), resource_type: PhantomData } }
    fn null() -> ResourceRef<R> { ResourceRef { index: None, resource_type: PhantomData } }
}

pub trait Element {
    fn measure(&self, resources: &Resources, children: &[BoundingBox]) -> BoundingBox {
        let box_style = resources.get_style::<BoxStyle>();
        BoxStyle::measure(&box_style, children)
    }
    fn arrange(&mut self, resources: &Resources, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
        let box_style = resources.get_style::<BoxStyle>();
        BoxStyle::arrange(&box_style, bounds, children)
    }
    fn display(&self, resources: &Resources, bounds: BoundingBox, input_state: InputState, list: &mut DisplayList) {}

    fn type_id(&self) -> TypeId where Self: 'static { TypeId::of::<Self>() }
}

impl Element {
    unsafe fn downcast_mut_unchecked<E: Element>(&mut self) -> &mut E {
        &mut *(self as *mut Element as *mut E)
    }
}

pub struct UI {
    width: f32,
    height: f32,

    next_id: usize,
    elements: HashMap<ElementRef, Box<Element>>,
    root: ElementRef,
    focus: Option<ElementRef>,
    dragging: Option<ElementRef>,
    parents: HashMap<ElementRef, ElementRef>,
    children: HashMap<ElementRef, Vec<ElementRef>>,
    slots: HashMap<ElementRef, ElementRef>,
    layout: HashMap<ElementRef, BoundingBox>,
    next_class_id: usize,
    classes: HashMap<ClassRef, HashSet<ElementRef>>,
    element_classes: HashMap<ElementRef, Vec<ClassRef>>,

    receivers: HashMap<ElementRef, AnyMap>,
    listeners: HashMap<ElementRef, AnyMap>,

    global_styles: AnyMap,
    global_element_styles: HashMap<TypeId, AnyMap>,
    class_styles: HashMap<ClassRef, AnyMap>,
    element_styles: HashMap<ElementRef, AnyMap>,

    next_resource_id: usize,
    resources: AnyMap,

    input_state: InputState,
    mouse_position_captured: bool,
}

impl UI {
    pub fn new(width: f32, height: f32) -> UI {
        let mut ui = UI {
            width: width,
            height: height,

            next_id: 1,
            elements: HashMap::new(),
            root: 0,
            focus: None,
            dragging: None,
            parents: HashMap::new(),
            children: HashMap::new(),
            slots: HashMap::new(),
            layout: HashMap::new(),
            next_class_id: 0,
            classes: HashMap::new(),
            element_classes: HashMap::new(),

            receivers: HashMap::new(),
            listeners: HashMap::new(),

            global_styles: AnyMap::new(),
            global_element_styles: HashMap::new(),
            class_styles: HashMap::new(),
            element_styles: HashMap::new(),

            next_resource_id: 0,
            resources: AnyMap::new(),

            input_state: InputState {
                mouse_position: Point { x: -1.0, y: -1.0 },
                mouse_drag_origin: None,
                mouse_left_pressed: false,
                mouse_middle_pressed: false,
                mouse_right_pressed: false,
                modifiers: Default::default(),
            },
            mouse_position_captured: false,
        };

        ui.hookup_element(0);
        ui.setup_element_styles(0);
        ui.elements.insert(0, Box::new(Stack));

        ui
    }

    /* size */

    pub fn get_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }

    /* element tree */

    pub fn place_root<'a, E, I>(&'a mut self, install: I) -> ElementRef where E: Element + 'static, I: FnOnce(Context<E>) -> E {
        let root = self.root;

        self.remove_element(root);

        self.hookup_element(root);
        self.setup_element_styles(root);
        let element = install(Context::new(self, root));
        self.elements.insert(root, Box::new(element));

        root
    }

    pub fn get_slot<'a>(&'a mut self, element: ElementRef) -> Slot<'a> {
        let slot = *self.slots.get(&element).expect("element does not have a slot");
        Slot::new(self, slot)
    }

    pub fn register_slot(&mut self, element: ElementRef, slot: ElementRef) {
        self.slots.insert(element, slot);
    }

    fn add_child<E, I>(&mut self, parent: ElementRef, install: I) -> ElementRef where E: Element + 'static, I: FnOnce(Context<E>) -> E {
        let child_id = self.get_next_id();
        self.hookup_element(child_id);
        self.setup_element_styles(child_id);
        self.children.get_mut(&parent).expect("invalid parent id").push(child_id);
        self.parents.insert(child_id, parent);

        let element = install(Context::new(self, child_id));
        self.elements.insert(child_id, Box::new(element));


        child_id
    }

    fn insert_child<E, I>(&mut self, parent: ElementRef, index: usize, install: I) -> ElementRef where E: Element + 'static, I: FnOnce(Context<E>) -> E {
        let child_id = self.get_next_id();
        self.hookup_element(child_id);
        self.setup_element_styles(child_id);
        self.children.get_mut(&parent).expect("invalid parent id").insert(index, child_id);
        self.parents.insert(child_id, parent);

        let element = install(Context::new(self, child_id));
        self.elements.insert(child_id, Box::new(element));

        child_id
    }

    fn remove_child(&mut self, parent: ElementRef, index: usize) {
        let child_id = self.children.get_mut(&parent).expect("invalid parent id").remove(index);
        self.remove_element(child_id);
    }

    fn get_child(&self, parent: ElementRef, index: usize) -> Option<ElementRef> {
        self.children.get(&parent).expect("invalid parent id").get(index).map(|child| *child)
    }

    fn get_next_id(&mut self) -> ElementRef {
        let element_id = self.next_id;
        self.next_id += 1;
        element_id
    }

    fn remove_element(&mut self, id: usize) {
        self.unhook_element(id);
        self.remove_element_styles(id);
    }

    fn hookup_element(&mut self, element: usize) {
        self.children.insert(element, Vec::new());
        self.layout.insert(element, BoundingBox::new(0.0, 0.0, 0.0, 0.0));
        self.receivers.insert(element, AnyMap::new());
        self.listeners.insert(element, AnyMap::new());
    }

    fn setup_element_styles(&mut self, element: usize) {
        self.element_classes.insert(element, Vec::new());
        self.element_styles.insert(element, AnyMap::new());
    }

    fn unhook_element(&mut self, element: usize) {
        self.elements.remove(&element);
        self.layout.remove(&element);
        self.receivers.remove(&element);
        self.listeners.remove(&element);
        let children = self.children.remove(&element).expect("invalid element id");
        for child in children {
            self.remove_element(child);
        }
    }

    fn remove_element_styles(&mut self, element: usize) {
        for class in self.element_classes.get(&element).expect("invalid element id") {
            self.classes.get_mut(class).expect("invalid class id").remove(&element);
        }
        self.element_classes.remove(&element);
        self.element_styles.remove(&element);
    }

    /* classes */

    pub fn new_class(&mut self) -> ClassRef {
        let class_id = self.next_class_id;
        self.classes.insert(class_id, HashSet::new());
        self.class_styles.insert(class_id, AnyMap::new());
        self.next_class_id += 1;
        class_id
    }

    pub fn delete_class(&mut self, class: ClassRef) {
        for element in self.classes.get(&class).expect("invalid class id") {
            self.element_classes.get_mut(&element).expect("invalid element id").retain(|c| *c != class);
        }
        self.classes.remove(&class);
        self.class_styles.remove(&class);
    }

    pub fn add_class(&mut self, element: usize, class: ClassRef) {
        self.classes.get_mut(&class).expect("invalid class id").insert(element);
        self.element_classes.get_mut(&element).expect("invalid element id").push(class);
    }

    pub fn remove_class(&mut self, element: usize, class: ClassRef) {
        self.classes.get_mut(&class).expect("invalid class id").remove(&element);
        self.element_classes.get_mut(&element).expect("invalid element id").retain(|c| *c != class);
    }

    /* styles */

    pub fn set_global_style<P: Patch>(&mut self, style: P::PatchType) {
        self.global_styles.entry::<P::PatchType>()
            .or_insert_with(|| P::PatchType::default())
            .merge(&style);
    }

    pub fn unset_global_style<P: Patch>(&mut self) {
        self.global_styles.remove::<P::PatchType>();
    }

    pub fn set_global_element_style<W: Element + 'static, P: Patch>(&mut self, style: P::PatchType) {
        self.global_element_styles.entry(TypeId::of::<W>())
            .or_insert_with(|| AnyMap::new())
            .entry::<P::PatchType>()
            .or_insert_with(|| P::PatchType::default())
            .merge(&style);
    }

    pub fn unset_global_element_style<W: Element + 'static, P: Patch>(&mut self) {
        if let Some(map) = self.global_element_styles.get_mut(&TypeId::of::<W>()) {
            map.remove::<P::PatchType>();
        }
    }

    pub fn set_class_style<P: Patch>(&mut self, class: ClassRef, style: P::PatchType) {
        self.class_styles.get_mut(&class).expect("invalid class id").entry::<P::PatchType>()
            .or_insert_with(|| P::PatchType::default())
            .merge(&style);
    }

    pub fn unset_class_style<P: Patch>(&mut self, class: ClassRef) {
        self.class_styles.get_mut(&class).expect("invalid class id").remove::<P::PatchType>();
    }

    pub fn set_element_style<P: Patch>(&mut self, element: usize, style: P::PatchType) {
        self.element_styles.get_mut(&element).expect("invalid element id").entry::<P::PatchType>()
            .or_insert_with(|| P::PatchType::default())
            .merge(&style);
    }

    pub fn unset_element_style<P: Patch>(&mut self, element: usize) {
        self.element_styles.get_mut(&element).expect("invalid element id").remove::<P::PatchType>();
    }

    /* resources */

    pub fn add_resource<R: 'static>(&mut self, resource: R) -> ResourceRef<R> {
        let resource_id = self.next_resource_id;
        self.resources.entry::<HashMap<usize, R>>()
            .or_insert_with(|| HashMap::new())
            .insert(resource_id, resource);
        self.next_resource_id += 1;
        ResourceRef { index: Some(resource_id), resource_type: PhantomData }
    }

    pub fn get_resource<R: 'static>(&mut self, resource_ref: ResourceRef<R>) -> &R {
         &self.resources.get::<HashMap<usize, R>>().expect("invalid resource id")[&resource_ref.index.expect("invalid resource id")]
    }

    pub fn remove_resource<R: 'static>(&mut self, resource_ref: ResourceRef<R>) -> R {
        self.resources.get_mut::<HashMap<usize, R>>().expect("invalid resource id").remove(&resource_ref.index.expect("invalid resource id")).expect("invalid resource id")
    }

    /* event handling */

    pub fn send<M: 'static>(&mut self, receiver: ElementRef, message: M) {
        if let Some(callback) = self.receivers.get_mut(&receiver).expect("invalid element id").remove::<Box<Fn(&mut UI, M)>>() {
            callback(self, message);
            self.receivers.get_mut(&receiver).expect("invalid element id").insert::<Box<Fn(&mut UI, M)>>(callback);
        }
    }

    fn receive<E: Element + 'static, M: 'static, F: Fn(&mut E, Context<E>, M) + 'static>(&mut self, receiver: ElementRef, callback: F) {
        self.receivers.get_mut(&receiver).expect("invalid element id").insert::<Box<Fn(&mut UI, M)>>(Box::new(move |ui, message| {
            let mut element = ui.elements.remove(&receiver).unwrap();
            callback(unsafe { element.downcast_mut_unchecked::<E>() }, Context::new(ui, receiver), message);
            ui.elements.insert(receiver, element);
        }));
    }

    fn fire<Ev: 'static>(&mut self, source: ElementRef, event: Ev) {
        if let Some(callback) = self.listeners.get_mut(&source).expect("invalid element id").remove::<Box<Fn(&mut UI, Ev)>>() {
            callback(self, event);
            self.listeners.get_mut(&source).expect("invalid element id").insert::<Box<Fn(&mut UI, Ev)>>(callback);
        }
    }

    fn listen<E: Element + 'static, Ev: 'static, F: Fn(&mut E, Context<E>, Ev) + 'static>(&mut self, listener: ElementRef, source: ElementRef, callback: F) {
        self.listeners.get_mut(&source).expect("invalid element id").insert::<Box<Fn(&mut UI, Ev)>>(Box::new(move |ui, event| {
            let mut element = ui.elements.remove(&listener).unwrap();
            callback(unsafe { element.downcast_mut_unchecked::<E>() }, Context::new(ui, listener), event);
            ui.elements.insert(listener, element);
        }));
    }

    pub fn set_modifiers(&mut self, modifiers: KeyboardModifiers) {
        self.input_state.modifiers = modifiers;
    }

    pub fn handle(&mut self, ev: InputEvent) -> UIEventResponse {
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
                    if self.layout[&self.root].contains_point(position) {
                        Some(self.find_element(self.root, position))
                    } else {
                        None
                    }
                // }
            },
            InputEvent::KeyPress { .. } | InputEvent::KeyRelease { .. } | InputEvent::TextInput { .. } => {
                self.focus.or(Some(self.root))
            }
            _ => {
                Some(self.root)
            }
        };

        if let Some(handler) = handler {
            let mut handler = handler;
            while self.parents.contains_key(&handler) && !self.receivers.get(&handler).expect("invalid element id").contains::<Box<Fn(&mut UI, InputEvent)>>() {
                handler = *self.parents.get(&handler).expect("invalid element id");
            }
            self.send(handler, ev);
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
        for child in self.children[&parent].iter() {
            if self.layout[child].contains_point(point) {
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
        self.elements.get(&element).expect("invalid element id").display(&Resources::new(element, self.elements[&element].type_id(), &self), *self.layout.get(&element).expect("invalid element id"), self.input_state, list);
        if let Some(children) = self.children.get(&element) {
            for child in children {
                self.display_element(*child, list);
            }
        }
    }

    /* layout */

    pub fn layout(&mut self) {
        let (_bounding_box, mut tree) = self.measure(self.root);
        let root = self.root;
        let bounds = BoundingBox::new(0.0, 0.0, self.width, self.height);
        self.arrange(root, bounds, &mut tree);
    }

    fn measure(&self, element: ElementRef) -> (BoundingBox, BoundingBoxTree) {
        let mut child_boxes = Vec::new();
        let mut child_trees = Vec::new();
        if let Some(children) = self.children.get(&element) {
            child_boxes.reserve(children.len());
            child_trees.reserve(children.len());
            for child in children {
                let (child_box, child_tree) = self.measure(*child);
                child_boxes.push(child_box);
                child_trees.push(child_tree);
            }
        }
        let bounding_box = self.elements.get(&element).expect("invalid element id").measure(&Resources::new(element, self.elements[&element].type_id(), &self), &child_boxes[..]);
        (bounding_box, BoundingBoxTree { boxes: child_boxes, trees: child_trees })
    }

    fn arrange(&mut self, element: ElementRef, bounds: BoundingBox, tree: &mut BoundingBoxTree) {
        let bounding_box = {
            let resources = Resources {
                element: element,
                element_type_id: self.elements[&element].type_id(),
                element_classes: &self.element_classes[&element],

                global_styles: &self.global_styles,
                global_element_styles: &self.global_element_styles,
                class_styles: &self.class_styles,
                element_styles: &self.element_styles,

                resources: &self.resources,
            };
            self.elements.get_mut(&element).expect("invalid element id").arrange(&resources, bounds, &mut tree.boxes[..])
        };
        self.layout.insert(element, bounding_box);
        if let Some(children) = self.children.get(&element).map(|children| children.clone()) {
            for (i, child) in children.iter().enumerate() {
                self.arrange(*child, tree.boxes[i], &mut tree.trees[i]);
            }
        }
    }
}

pub struct Context<'a, E: Element + 'static> {
    ui: &'a mut UI,
    element: ElementRef,
    phantom_data: PhantomData<E>,
}

impl<'a, E: Element + 'static> Context<'a, E> {
    fn new(ui: &'a mut UI, element: ElementRef) -> Context<'a, E> {
        Context {
            ui: ui,
            element: element,
            phantom_data: PhantomData
        }
    }

    pub fn get_self(&self) -> ElementRef {
        self.element
    }

    pub fn subtree<'b>(&'b mut self) -> Slot<'b> {
        Slot::new(self.ui, self.element)
    }

    pub fn get_slot<'b>(&'b mut self, element: ElementRef) -> Slot<'b> {
        self.ui.get_slot(element)
    }

    pub fn register_slot(&mut self, element: ElementRef) {
        self.ui.register_slot(self.element, element);
    }

    pub fn send<M: 'static>(&mut self, element: ElementRef, message: M) {
        self.ui.send::<M>(element, message);
    }

    pub fn receive<M: 'static, F: Fn(&mut E, Context<E>, M) + 'static>(&mut self, callback: F) {
        self.ui.receive::<E, M, F>(self.element, callback);
    }

    pub fn fire<Ev: Clone + 'static>(&mut self, event: Ev) {
        self.ui.fire::<Ev>(self.element, event);
    }

    pub fn listen<Ev: 'static, F: Fn(&mut E, Context<E>, Ev) + 'static>(&mut self, element: ElementRef, callback: F) {
        self.ui.listen::<E, Ev, F>(self.element, element, callback);
    }

    pub fn set_element_style<P: Patch>(&mut self, element: usize, style: P::PatchType) {
        self.ui.set_element_style::<P>(element, style);
    }

    pub fn get_input_state(&self) -> InputState {
        self.ui.input_state
    }

    pub fn resources(&self) -> Resources {
        Resources::new(self.element, TypeId::of::<E>(), &self.ui)
    }
}

pub struct Slot<'a> {
    ui: &'a mut UI,
    element: ElementRef,
}

impl<'a> Slot<'a> {
    fn new(ui: &'a mut UI, element: ElementRef) -> Slot<'a> {
        Slot {
            ui: ui,
            element: element,
        }
    }

    pub fn add_child<E, I>(&mut self, install: I) -> ElementRef where E: Element + 'static, I: FnOnce(Context<E>) -> E {
        self.ui.add_child(self.element, install)
    }

    pub fn insert_child<E, I>(&mut self, index: usize, install: I) -> ElementRef where E: Element + 'static, I: FnOnce(Context<E>) -> E {
        self.ui.insert_child(self.element, index, install)
    }

    pub fn remove_child(&mut self, index: usize) {
        self.ui.remove_child(self.element, index)
    }

    pub fn get_child(&self, index: usize) -> Option<ElementRef> {
        self.ui.get_child(self.element, index)
    }
}

struct BoundingBoxTree {
    boxes: Vec<BoundingBox>,
    trees: Vec<BoundingBoxTree>,
}

#[derive(Copy, Clone, Debug)]
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
    pub mouse_position: Point,
    pub mouse_drag_origin: Option<Point>,
    pub mouse_left_pressed: bool,
    pub mouse_middle_pressed: bool,
    pub mouse_right_pressed: bool,
    pub modifiers: KeyboardModifiers,
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

pub struct EventResponse {
    capture_keyboard: bool,
    capture_mouse: bool,
    capture_mouse_position: bool,
    mouse_cursor: MouseCursor,
}

impl Default for EventResponse {
    fn default() -> EventResponse {
        EventResponse {
            capture_keyboard: false,
            capture_mouse: false,
            capture_mouse_position: false,
            mouse_cursor: MouseCursor::Default,
        }
    }
}

pub struct Resources<'a> {
    element: usize,
    element_type_id: TypeId,
    element_classes: &'a Vec<ClassRef>,

    global_styles: &'a AnyMap,
    global_element_styles: &'a HashMap<TypeId, AnyMap>,
    class_styles: &'a HashMap<ClassRef, AnyMap>,
    element_styles: &'a HashMap<usize, AnyMap>,

    resources: &'a AnyMap,
}

impl<'a> Resources<'a> {
    fn new(element: ElementRef, type_id: TypeId, ui: &UI) -> Resources {
        Resources {
            element: element,
            element_type_id: type_id,
            element_classes: &ui.element_classes[&element],

            global_styles: &ui.global_styles,
            global_element_styles: &ui.global_element_styles,
            class_styles: &ui.class_styles,
            element_styles: &ui.element_styles,

            resources: &ui.resources,
        }
    }

    pub fn get_style<P: Patch + Default>(&self) -> P {
        let mut style: P = Default::default();
        if let Some(patch) = self.global_styles.get::<P::PatchType>() {
            style.patch(patch);
        }
        if let Some(map) = self.global_element_styles.get(&self.element_type_id) {
            if let Some(patch) = map.get::<P::PatchType>() {
                style.patch(patch);
            }
        }
        for class in self.element_classes.iter() {
            if let Some(patch) = self.class_styles[&class].get::<P::PatchType>() {
                style.patch(patch);
            }
        }
        if let Some(patch) = self.element_styles[&self.element].get::<P::PatchType>() {
            style.patch(patch);
        }
        style
    }

    pub fn get_resource<R: 'static>(&self, resource_ref: ResourceRef<R>) -> &R {
        &self.resources.get::<HashMap<usize, R>>().expect("invalid resource id")[&resource_ref.index.expect("invalid resource id")]
    }
}


macro_rules! style {
    (struct $style:ident { $($field:ident: $type:ty,)* }, $patch:ident) => {
        pub struct $style {
            pub $($field: $type,)*
        }

        pub struct $patch {
            $($field: Option<$type>,)*
        }

        impl $style {
            $(
                pub fn $field(value: $type) -> $patch {
                    $patch { $field: Some(value), ..Default::default() }
                }
            )*
        }

        impl $patch {
            $(
                pub fn $field(mut self, value: $type) -> $patch {
                    self.$field = Some(value);
                    self
                }
            )*
        }

        impl Default for $patch {
            fn default() -> $patch {
                $patch {
                    $($field: None,)*
                }
            }
        }

        impl Patch for $style {
            type PatchType = $patch;
            fn patch(&mut self, patch: &$patch) {
                $(
                    if let Some($field) = patch.$field.clone() {
                        self.$field = $field;
                    }
                )*
            } 
        }

        impl Merge for $patch {
            fn merge(&mut self, other: &$patch) {
                $(
                    if other.$field.is_some() {
                        self.$field = other.$field.clone();
                    }
                )*
            }
        }
    }
}


pub trait Patch {
    type PatchType: Default + Merge + 'static;
    fn patch(&mut self, patch: &Self::PatchType);
}

pub trait Merge {
    fn merge(&mut self, other: &Self);
}


style! {
    struct BoxStyle {
        padding: f32,
        min_width: f32,
        min_height: f32,
        max_width: f32,
        max_height: f32,
        h_align: Align,
        v_align: Align,
        color: [f32; 4],
    },
    BoxStylePatch
}

impl Default for BoxStyle {
    fn default() -> BoxStyle {
        BoxStyle {
            padding: 0.0,
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
            h_align: Align::Beginning,
            v_align: Align::Beginning,
            color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

impl BoxStyle {
    fn measure(style: &BoxStyle, children: &[BoundingBox]) -> BoundingBox {
        let mut width = 0.0f32;
        let mut height = 0.0f32;
        for child_box in children {
            width = width.max(child_box.size.x);
            height = height.max(child_box.size.y);
        }

        width += 2.0 * style.padding;
        height += 2.0 * style.padding;

        BoundingBox { pos: Point::new(0.0, 0.0), size: Point::new(width, height) }
    }
    fn arrange(style: &BoxStyle, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
        for child_box in children.iter_mut() {
            child_box.pos = bounds.pos;
            child_box.pos.x += style.padding;
            child_box.pos.y += style.padding;
            child_box.size.x = bounds.size.x - style.padding * 2.0;
            child_box.size.y = bounds.size.y - style.padding * 2.0;
        }

        bounds
    }
}


pub struct Stack;

impl Stack {
    pub fn install(mut ctx: Context<Stack>) -> Stack {
        let id = ctx.get_self();
        ctx.register_slot(id);
        Stack
    }

    fn main_cross(&self, axis: Axis, point: Point) -> (f32, f32) { match axis { Axis::Horizontal => (point.x, point.y), Axis::Vertical => (point.y, point.x) } }
    fn x_y(&self, axis: Axis, main: f32, cross: f32) -> Point { match axis { Axis::Horizontal => Point::new(main, cross), Axis::Vertical => Point::new(cross, main) } }
}

style! {
    struct StackStyle {
        spacing: f32,
        axis: Axis,
        grow: Grow,
    },
    StackStylePatch
}

impl Default for StackStyle {
    fn default() -> StackStyle {
        StackStyle {
            spacing: 0.0,
            axis: Axis::Horizontal,
            grow: Grow::None,
        }
    }
}

impl Element for Stack {
    fn measure(&self, resources: &Resources, children: &[BoundingBox]) -> BoundingBox {
        let box_style = resources.get_style::<BoxStyle>();
        let stack_style = resources.get_style::<StackStyle>();

        let mut main = 0.0f32;
        let mut cross = 0.0f32;
        for child_box in children {
            let (child_main, child_cross) = self.main_cross(stack_style.axis, child_box.size);
            main += child_main;
            cross = cross.max(child_cross);
        }

        main += 2.0 * box_style.padding + stack_style.spacing * (children.len() as i32 - 1).max(0) as f32;
        cross += 2.0 * box_style.padding;

        let mut size = self.x_y(stack_style.axis, main, cross);
        size.x = size.x.max(box_style.min_width).min(box_style.max_width);
        size.y = size.y.max(box_style.min_height).min(box_style.max_height);

        BoundingBox { pos: Point::new(0.0, 0.0), size: size }
    }

    fn arrange(&mut self, resources: &Resources, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
        let box_style = resources.get_style::<BoxStyle>();
        let stack_style = resources.get_style::<StackStyle>();

        let (main_offset, cross_offset) = self.main_cross(stack_style.axis, bounds.pos);
        let (main_max, cross_max) = self.main_cross(stack_style.axis, bounds.size);
        let mut children_main = 0.0;
        for child_box in children.iter() {
            children_main += self.main_cross(stack_style.axis, child_box.size).0;
        }
        let extra = main_max - 2.0 * box_style.padding - stack_style.spacing * (children.len() as i32 - 1).max(0) as f32 - children_main;
        let child_cross = cross_max - 2.0 * box_style.padding;

        match stack_style.grow {
            Grow::None => {
                for child_box in children.iter_mut() {
                    let (child_main, _child_cross) = self.main_cross(stack_style.axis, child_box.size);
                    child_box.size = self.x_y(stack_style.axis, child_main, child_cross);
                }
            }
            Grow::Equal => {
                let children_len = children.len() as f32;
                for child_box in children.iter_mut() {
                    let (child_main, _child_cross) = self.main_cross(stack_style.axis, child_box.size);
                    child_box.size = self.x_y(stack_style.axis, child_main + extra / children_len, child_cross);
                }
            }
            Grow::Ratio(amounts) => {
                let total: f32 = amounts.iter().sum();
                for (i, child_box) in children.iter_mut().enumerate() {
                    let (child_main, _child_cross) = self.main_cross(stack_style.axis, child_box.size);
                    child_box.size = self.x_y(stack_style.axis, child_main + extra * amounts[i] / total, child_cross);
                }
            }
        }

        let mut main_offset = main_offset + box_style.padding;
        let cross_offset = cross_offset + box_style.padding;
        for child_box in children.iter_mut() {
            let child_main = self.main_cross(stack_style.axis, child_box.size).0;
            child_box.pos = self.x_y(stack_style.axis, main_offset, cross_offset);
            child_box.size = self.x_y(stack_style.axis, child_main, child_cross);
            main_offset += child_main + stack_style.spacing;
        }

        bounds
    }

    fn display(&self, resources: &Resources, bounds: BoundingBox, input_state: InputState, list: &mut DisplayList) {
        let box_style = resources.get_style::<BoxStyle>();

        // let color = [0.15, 0.18, 0.23, 1.0];
        list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: box_style.color });
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum Align {
    Beginning,
    Center,
    End
}

#[derive(Copy, Clone, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, PartialEq)]
pub enum Grow {
    None,
    Equal,
    Ratio(Vec<f32>),
}


style! {
    struct TextStyle {
        font: ResourceRef<Font<'static>>,
        scale: Scale,
    },
    TextStylePatch
}

impl Default for TextStyle {
    fn default() -> TextStyle {
        TextStyle {
            font: ResourceRef::null(),
            scale: Scale::uniform(14.0),
        }
    }
}

pub struct Label {
    text: Cow<'static, str>,
    glyphs: Vec<PositionedGlyph<'static>>,
    size: Point,
}

impl Label {
    pub fn with_text<S>(text: S) -> impl FnOnce(Context<Label>) -> Label where S: Into<Cow<'static, str>> {
        move |mut ctx| {
            let label = {
                let resources = ctx.resources();
                let box_style = resources.get_style::<BoxStyle>();
                let text_style = resources.get_style::<TextStyle>();
                let font = resources.get_resource::<Font>(text_style.font);

                let mut label = Label {
                    text: text.into(),
                    glyphs: Vec::new(),
                    size: Point::new(0.0, 0.0),
                };
                label.size = label.layout(text_style.scale, font, Point::new(box_style.padding, box_style.padding));

                label
            };

            ctx.receive(|myself: &mut Label, ctx, s: String| {
                myself.text = s.into();
            });

            ctx.receive(|myself: &mut Label, ctx, s: &'static str| {
                myself.text = s.into();
            });

            label
        }
    }

    fn layout(&mut self, scale: Scale, font: &Font, pos: Point) -> Point {
        use unicode_normalization::UnicodeNormalization;
        self.glyphs.clear();

        let v_metrics = font.v_metrics(scale);
        let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let mut caret = point(pos.x, pos.y + v_metrics.ascent);
        let mut last_glyph_id = None;
        for c in self.text.nfc() {
            if c.is_control() {
                match c {
                    '\r' => {
                        // caret = point(pos.x, caret.y + advance_height);
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
            // if let Some(bb) = glyph.pixel_bounding_box() {
            //     if bb.max.x > (pos.x + size.x) as i32 {
            //         caret = point(pos.x, caret.y + advance_height);
            //         glyph = glyph.into_unpositioned().positioned(caret);
            //         last_glyph_id = None;
            //     }
            // }
            caret.x += glyph.unpositioned().h_metrics().advance_width;
            self.glyphs.push(glyph.standalone());
        }

        Point::new(caret.x - pos.x, v_metrics.ascent - v_metrics.descent)
    }
}

impl Element for Label {
    fn measure(&self, resources: &Resources, children: &[BoundingBox]) -> BoundingBox {
        let box_style = resources.get_style::<BoxStyle>();
        BoundingBox { pos: Point::new(0.0, 0.0), size: self.size + Point::new(2.0 * box_style.padding, 2.0 * box_style.padding) }
    }

    fn arrange(&mut self, resources: &Resources, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
        let box_style = resources.get_style::<BoxStyle>();
        let text_style = resources.get_style::<TextStyle>();
        let font = resources.get_resource::<Font>(text_style.font);
        self.size = self.layout(text_style.scale, font, bounds.pos + Point::new(box_style.padding, box_style.padding));
        BoundingBox { pos: bounds.pos, size: self.size + Point::new(2.0 * box_style.padding, 2.0 * box_style.padding) }
    }

    fn display(&self, resources: &Resources, bounds: BoundingBox, input_state: InputState, list: &mut DisplayList) {
        let box_style = resources.get_style::<BoxStyle>();

        list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: box_style.color });
        for glyph in self.glyphs.iter() {
            list.glyph(glyph.standalone());
        }
    }
}


pub struct Button;

impl Button {
    pub fn install(mut ctx: Context<Button>) -> Button {
        let id = ctx.get_self();
        ctx.register_slot(id);

        ctx.receive(Button::handle);

        Button
    }

    fn handle(&mut self, mut ctx: Context<Button>, evt: InputEvent) {
        if let InputEvent::MouseRelease { button: MouseButton::Left } = evt {
            ctx.fire(&ClickEvent);
        }
    }
}

pub struct ClickEvent;

impl Element for Button {
    fn display(&self, resources: &Resources, bounds: BoundingBox, input_state: InputState, list: &mut DisplayList) {
        let box_style = resources.get_style::<BoxStyle>();

        let mut color = [0.15, 0.18, 0.23, 1.0];
        if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
            if bounds.contains_point(mouse_drag_origin) {
                color = [0.02, 0.2, 0.6, 1.0];
            }
        } else if bounds.contains_point(input_state.mouse_position) {
            color = [0.3, 0.4, 0.5, 1.0];
        }

        list.rect(Rect { x: bounds.pos.x, y: bounds.pos.y, w: bounds.size.x, h: bounds.size.y, color: color });
    }
}


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


// pub struct IntegerInput {
//     value: i32,
//     new_value: Option<i32>,
//     on_change: Option<Box<Fn(i32)>>,
//     format: Option<Box<Fn(i32) -> String>>,
//     container: Rc<RefCell<Container>>,
//     label: Rc<RefCell<Label>>,
// }

// impl IntegerInput {
//     pub fn new(value: i32, font: Rc<Font<'static>>) -> Rc<RefCell<IntegerInput>> {
//         let label = Label::new(&value.to_string(), font);
//         let container = Container::new(label.clone());
//         container.borrow_mut().get_style().padding = 2.0;
//         Rc::new(RefCell::new(IntegerInput { value: value, new_value: None, on_change: None, format: None, container: container, label: label }))
//     }

//     pub fn set_value(&mut self, value: i32) {
//         self.value = value;
//         self.render_text(value);
//     }

//     pub fn on_change<F>(&mut self, callback: F) where F: 'static + Fn(i32) {
//         self.on_change = Some(Box::new(callback));
//     }

//     pub fn format<F>(&mut self, callback: F) where F: 'static + Fn(i32) -> String {
//         self.label.borrow_mut().set_text(&callback(self.value));
//         self.format = Some(Box::new(callback));
//     }

//     fn render_text(&mut self, value: i32) {
//         let text = if let Some(ref format) = self.format {
//             format(value)
//         } else {
//             value.to_string()
//         };
//         self.label.borrow_mut().set_text(&text);
//     }
// }

// impl Element for IntegerInput {
//     fn set_container_size(&mut self, width: Option<f32>, height: Option<f32>) {
//         self.container.borrow_mut().set_container_size(width, height);
//     }

//     fn get_min_size(&self) -> (f32, f32) {
//         self.container.borrow().get_min_size()
//     }

//     fn get_size(&self) -> (f32, f32) {
//         self.container.borrow().get_size()
//     }

//     fn handle_event(&mut self, ev: InputEvent, input_state: InputState) -> EventResponse {
//         match ev {
//             InputEvent::CursorMoved { position } => {
//                 if let Some(mouse_drag_origin) = input_state.mouse_drag_origin {
//                     let dy = -(input_state.mouse_position.y - mouse_drag_origin.y);
//                     let new_value = self.value + (dy / 8.0) as i32;
//                     self.new_value = Some(new_value);
//                     self.render_text(new_value);
//                     if let Some(ref on_change) = self.on_change {
//                         on_change(new_value);
//                     }
//                     return EventResponse {
//                         capture_mouse: true,
//                         capture_mouse_position: true,
//                         mouse_cursor: MouseCursor::NoneCursor,
//                         ..Default::default()
//                     };
//                 }
//             }
//             InputEvent::MouseRelease { button: MouseButton::Left } => {
//                 if let Some(new_value) = self.new_value {
//                     self.value = new_value;
//                     self.new_value = None;
//                 }
//             }
//             _ => {}
//         }

//         Default::default()
//     }

//     fn display(&self, input_state: InputState, list: &mut DisplayList) {
//         self.container.borrow().display(input_state, list);
//     }
// }


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
