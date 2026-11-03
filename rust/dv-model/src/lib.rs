//! The design model every DV tool reads: classes, their UVM role, properties
//! and methods, with source locations.
//!
//! [`Design::load`] takes the same compile options as `uvmcheck` and returns
//! an owned snapshot; nothing borrows from the front end afterwards. Today the
//! front end is slang behind `sv-slang`; tools do not see that and will not
//! notice when layers move to native Rust.

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::c_char;
use std::time::Instant;

use sv_slang as ffi;

pub type ClassId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UvmKind {
    None,
    Object,
    SequenceItem,
    Sequence,
    Component,
    Test,
    Env,
    Agent,
    Driver,
    Monitor,
    Sequencer,
    Scoreboard,
    Subscriber,
}

impl UvmKind {
    fn from_c(v: u8) -> Self {
        use UvmKind::*;
        [None, Object, SequenceItem, Sequence, Component, Test, Env, Agent, Driver, Monitor, Sequencer, Scoreboard, Subscriber]
            .get(v as usize)
            .copied()
            .unwrap_or(None)
    }

    /// Any `uvm_component`, whatever its role.
    pub fn is_component(self) -> bool {
        self as u8 >= UvmKind::Component as u8
    }

    pub fn as_str(self) -> &'static str {
        use UvmKind::*;
        match self {
            None => "none",
            Object => "object",
            SequenceItem => "sequence_item",
            Sequence => "sequence",
            Component => "component",
            Test => "test",
            Env => "env",
            Agent => "agent",
            Driver => "driver",
            Monitor => "monitor",
            Sequencer => "sequencer",
            Scoreboard => "scoreboard",
            Subscriber => "subscriber",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Local,
}

impl Visibility {
    fn from_c(v: u8) -> Self {
        match v {
            1 => Visibility::Protected,
            2 => Visibility::Local,
            _ => Visibility::Public,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseKind {
    None,
    Function,
    Task,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Loc {
    /// Index into [`Design::files`]; 0 is "no file".
    pub file: u32,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub ty: String,
    pub type_class: Option<ClassId>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_const: bool,
    pub is_rand: bool,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Method {
    pub name: String,
    pub visibility: Visibility,
    pub is_task: bool,
    pub is_virtual: bool,
    pub is_pure: bool,
    pub is_static: bool,
    pub phase: PhaseKind,
    pub loc: Loc,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub id: ClassId,
    pub name: String,
    pub scope: String,
    pub base_name: String,
    pub base: Option<ClassId>,
    pub uvm: UvmKind,
    pub is_virtual: bool,
    pub is_parameterized: bool,
    pub is_library: bool,
    pub loc: Loc,
    pub properties: Vec<Property>,
    pub methods: Vec<Method>,
}

/// Wall-clock cost of a load, by stage, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    pub parse_ms: f64,
    pub elaborate_ms: f64,
    /// Building the C++ model from the elaborated AST.
    pub model_ms: f64,
    /// Copying the model across the C ABI into Rust.
    pub copy_ms: f64,
}

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct Design {
    files: Vec<String>,
    classes: Vec<Class>,
    compile_errors: u32,
    timings: Timings,
}

/// Owns a C design and frees it on every exit path.
struct Handle(*mut ffi::uvmcheck_design);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { ffi::uvmcheck_design_free(self.0) }
    }
}

fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

fn loc(l: ffi::uvmcheck_loc) -> Loc {
    Loc { file: l.file, line: l.line, column: l.column }
}

fn id(v: u32) -> Option<ClassId> {
    (v != ffi::NONE).then_some(v)
}

impl Design {
    /// Loads a design from compile options as given to `uvmcheck`, e.g.
    /// `["+incdir+src", "src/uvm_pkg.sv"]` or `["-f", "files.f"]`.
    pub fn load<I, S>(args: I) -> Result<Design, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let abi = unsafe { ffi::uvmcheck_model_abi_version() };
        if abi != ffi::ABI_VERSION {
            return Err(Error(format!("libuvmcheck_model ABI {abi}, expected {}", ffi::ABI_VERSION)));
        }
        let args: Vec<CString> = args
            .into_iter()
            .map(|a| CString::new(a.as_ref()).map_err(|_| Error("argument contains a NUL byte".into())))
            .collect::<Result<_, _>>()?;
        let argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        let handle = Handle(unsafe { ffi::uvmcheck_design_load(argv.len() as i32, argv.as_ptr()) });
        let d = handle.0;
        let err = unsafe { ffi::uvmcheck_design_error(d) };
        if !err.is_null() {
            return Err(Error(text(err)));
        }

        let start = Instant::now();
        let files = (0..unsafe { ffi::uvmcheck_file_count(d) })
            .map(|i| text(unsafe { ffi::uvmcheck_file_name(d, i) }))
            .collect();
        let count = unsafe { ffi::uvmcheck_class_count(d) };
        let mut classes = Vec::with_capacity(count as usize);
        for cid in 0..count {
            let mut c = std::mem::MaybeUninit::<ffi::uvmcheck_class>::uninit();
            if unsafe { ffi::uvmcheck_get_class(d, cid, c.as_mut_ptr()) } == 0 {
                return Err(Error(format!("class {cid} missing")));
            }
            let c = unsafe { c.assume_init() };
            let mut properties = Vec::with_capacity(c.property_count as usize);
            for i in 0..c.property_count {
                let mut p = std::mem::MaybeUninit::<ffi::uvmcheck_property>::uninit();
                if unsafe { ffi::uvmcheck_get_property(d, cid, i, p.as_mut_ptr()) } == 0 {
                    return Err(Error(format!("property {i} of class {cid} missing")));
                }
                let p = unsafe { p.assume_init() };
                properties.push(Property {
                    name: text(p.name),
                    ty: text(p.type_),
                    type_class: id(p.type_class),
                    visibility: Visibility::from_c(p.visibility),
                    is_static: p.is_static != 0,
                    is_const: p.is_const != 0,
                    is_rand: p.is_rand != 0,
                    loc: loc(p.loc),
                });
            }
            let mut methods = Vec::with_capacity(c.method_count as usize);
            for i in 0..c.method_count {
                let mut m = std::mem::MaybeUninit::<ffi::uvmcheck_method>::uninit();
                if unsafe { ffi::uvmcheck_get_method(d, cid, i, m.as_mut_ptr()) } == 0 {
                    return Err(Error(format!("method {i} of class {cid} missing")));
                }
                let m = unsafe { m.assume_init() };
                methods.push(Method {
                    name: text(m.name),
                    visibility: Visibility::from_c(m.visibility),
                    is_task: m.is_task != 0,
                    is_virtual: m.is_virtual != 0,
                    is_pure: m.is_pure != 0,
                    is_static: m.is_static != 0,
                    phase: match m.phase {
                        1 => PhaseKind::Function,
                        2 => PhaseKind::Task,
                        _ => PhaseKind::None,
                    },
                    loc: loc(m.loc),
                });
            }
            classes.push(Class {
                id: cid,
                name: text(c.name),
                scope: text(c.scope),
                base_name: text(c.base_name),
                base: id(c.base),
                uvm: UvmKind::from_c(c.uvm),
                is_virtual: c.is_virtual != 0,
                is_parameterized: c.is_parameterized != 0,
                is_library: c.is_library != 0,
                loc: loc(c.loc),
                properties,
                methods,
            });
        }
        let copy_ms = start.elapsed().as_secs_f64() * 1e3;
        let timings = unsafe {
            Timings {
                parse_ms: ffi::uvmcheck_design_time_ms(d, 0),
                elaborate_ms: ffi::uvmcheck_design_time_ms(d, 1),
                model_ms: ffi::uvmcheck_design_time_ms(d, 2),
                copy_ms,
            }
        };
        let compile_errors = unsafe { ffi::uvmcheck_design_compile_errors(d) };
        Ok(Design { files, classes, compile_errors, timings })
    }

    pub fn classes(&self) -> &[Class] {
        &self.classes
    }

    pub fn class(&self, id: ClassId) -> &Class {
        &self.classes[id as usize]
    }

    pub fn find_class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    pub fn file_name(&self, loc: Loc) -> &str {
        self.files.get(loc.file as usize).map(String::as_str).unwrap_or("")
    }

    pub fn compile_errors(&self) -> u32 {
        self.compile_errors
    }

    pub fn timings(&self) -> Timings {
        self.timings
    }

    /// The class's base chain, nearest first, ending at the first base that is
    /// not in the model. Stops on a cycle.
    pub fn ancestors(&self, id: ClassId) -> impl Iterator<Item = &Class> {
        let mut next = self.class(id).base;
        let mut left = self.classes.len();
        std::iter::from_fn(move || {
            let cur = next.filter(|_| left > 0)?;
            left -= 1;
            let c = self.class(cur);
            next = c.base;
            Some(c)
        })
    }

    /// True when the class is `name` or derives from it.
    pub fn derives_from(&self, id: ClassId, name: &str) -> bool {
        let c = self.class(id);
        c.name == name || c.base_name == name || self.ancestors(id).any(|a| a.name == name || a.base_name == name)
    }

    /// Component-typed properties of a component: its likely children.
    pub fn child_components<'a>(&'a self, c: &'a Class) -> impl Iterator<Item = (&'a Property, &'a Class)> + 'a {
        c.properties.iter().filter_map(move |p| {
            let t = self.class(p.type_class?);
            t.uvm.is_component().then_some((p, t))
        })
    }
}
