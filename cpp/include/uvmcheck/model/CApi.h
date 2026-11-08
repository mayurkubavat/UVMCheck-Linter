/* C ABI of libuvmcheck_model: load a design, read its uvmcheck::model.
 *
 * Plain C so that Rust (crates/sv-slang), Python (ctypes) and any other tool can
 * use it with no C++ toolchain. Every string and record stays valid until
 * uvmcheck_design_free(). Ids are 0-based; UVMCHECK_NONE marks "no class".
 * The ABI version changes whenever a struct below changes.
 */
#ifndef UVMCHECK_MODEL_CAPI_H
#define UVMCHECK_MODEL_CAPI_H

#include <stdint.h>

#if defined(_WIN32)
#  define UVMCHECK_API __declspec(dllexport)
#else
#  define UVMCHECK_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define UVMCHECK_MODEL_ABI_VERSION 7u
#define UVMCHECK_NONE 0xFFFFFFFFu

typedef struct uvmcheck_design uvmcheck_design;

/* Values of uvmcheck_class.uvm, in uvmcheck::model::UvmKind order. */
enum {
    UVMCHECK_UVM_NONE = 0, UVMCHECK_UVM_OBJECT, UVMCHECK_UVM_SEQUENCE_ITEM, UVMCHECK_UVM_SEQUENCE,
    UVMCHECK_UVM_COMPONENT, UVMCHECK_UVM_TEST, UVMCHECK_UVM_ENV, UVMCHECK_UVM_AGENT,
    UVMCHECK_UVM_DRIVER, UVMCHECK_UVM_MONITOR, UVMCHECK_UVM_SEQUENCER, UVMCHECK_UVM_SCOREBOARD,
    UVMCHECK_UVM_SUBSCRIBER
};

typedef struct {
    uint32_t file, line, column;
    /* Code from a macro: file and line of the outermost expansion; 0 otherwise. */
    uint32_t exp_file, exp_line;
} uvmcheck_loc;

typedef struct {
    const char* name;
    const char* scope;
    const char* base_name;
    uint32_t base;
    uint8_t uvm;
    uint8_t is_virtual, is_parameterized, is_library;
    uvmcheck_loc loc;
    uint32_t property_count, method_count, constraint_count;
    uint32_t macro_count, typedef_count;
    uint8_t is_interface;
} uvmcheck_class;

typedef struct {
    const char* name;
    const char* type;
    uint32_t type_class;
    uint8_t visibility; /* 0 public, 1 protected, 2 local */
    uint8_t is_static, is_const, is_rand, is_virtual_interface;
    uvmcheck_loc loc;
} uvmcheck_property;

typedef struct {
    const char* name;
    uint8_t visibility;
    uint8_t is_task, is_virtual, is_pure, is_static;
    uint8_t phase; /* 0 none, 1 function phase, 2 task phase */
    uint8_t declared_virtual; /* has the `virtual` keyword itself */
    uint8_t is_builtin;       /* made by the compiler (randomize and the like) */
    uint8_t is_extern;        /* declared by a prototype; loc is the body outside the class */
    uint8_t returns_void;
    const char* return_type;
    uvmcheck_loc loc;
    uint32_t argument_count, local_count, call_count, assignment_count, timing_count;
    int32_t super_new_index;     /* top-level statement index of `super.new(...);`, or -1 */
    uvmcheck_loc super_new_loc;
    uint8_t calls_super_new;     /* super.new(...) anywhere in the body */
} uvmcheck_method;

/* Values of uvmcheck_argument.direction and .shape. */
enum { UVMCHECK_DIR_IN = 0, UVMCHECK_DIR_OUT, UVMCHECK_DIR_INOUT, UVMCHECK_DIR_REF };
enum {
    UVMCHECK_SHAPE_OTHER = 0, UVMCHECK_SHAPE_DYNAMIC_ARRAY, UVMCHECK_SHAPE_QUEUE,
    UVMCHECK_SHAPE_ASSOCIATIVE_ARRAY, UVMCHECK_SHAPE_FIXED_UNPACKED_ARRAY, UVMCHECK_SHAPE_UNPACKED_STRUCT
};

typedef struct {
    const char* name;
    const char* type;
    const char* canonical_type;
    uint32_t type_class;
    uint8_t direction, shape;
    uvmcheck_loc loc;
} uvmcheck_argument;

typedef struct {
    const char* name;
    const char* type;
    uint8_t is_virtual_interface;
    uvmcheck_loc loc;
} uvmcheck_local;

typedef struct {
    const char* name;
    uvmcheck_loc loc;
} uvmcheck_constraint;

/* A user macro used inside the class's text, by name without the backtick. */
typedef struct {
    const char* name;
    uvmcheck_loc loc;
} uvmcheck_macro;

/* A typedef declared in the class. */
typedef struct {
    const char* name;
    uvmcheck_loc loc;
} uvmcheck_typedef;

/* Values of uvmcheck_assignment.root_kind. */
enum { UVMCHECK_ROOT_OTHER = 0, UVMCHECK_ROOT_ARGUMENT, UVMCHECK_ROOT_LOCAL, UVMCHECK_ROOT_PROPERTY };

/* An assignment in a method body, in walk order. */
typedef struct {
    const char* lhs;       /* left side as written */
    const char* rhs;       /* right side as written */
    const char* root_name; /* argument, local or property the left side is rooted in */
    uvmcheck_loc loc;
    uint8_t is_non_blocking, root_kind;
} uvmcheck_assignment;

/* A call in a method body, in walk order (nested calls included). */
typedef struct {
    const char* name;         /* as written: "randomize", "$urandom" */
    const char* callee_scope; /* class (generic name) or package declaring the callee; "" if none */
    const char* receiver;     /* object called on, as written ("m_env.ap"); "" if none */
    uvmcheck_loc loc;
    uvmcheck_loc first_arg_loc;
    uint8_t is_system, callee_declared_virtual, callee_builtin, first_arg_is_null, via_super;
} uvmcheck_call;

UVMCHECK_API uint32_t uvmcheck_model_abi_version(void);

/* argv: compile options as given to uvmcheck, without the program name.
 * Never returns NULL; check uvmcheck_design_error(). */
UVMCHECK_API uvmcheck_design* uvmcheck_design_load(int argc, const char* const* argv);
UVMCHECK_API void uvmcheck_design_free(uvmcheck_design* d);
/* NULL when the design loaded (compile errors in the code do not count). */
UVMCHECK_API const char* uvmcheck_design_error(const uvmcheck_design* d);
UVMCHECK_API uint32_t uvmcheck_design_compile_errors(const uvmcheck_design* d);
/* stage: 0 parse, 1 elaborate, 2 model build. Milliseconds. */
UVMCHECK_API double uvmcheck_design_time_ms(const uvmcheck_design* d, int stage);

UVMCHECK_API uint32_t uvmcheck_file_count(const uvmcheck_design* d);
UVMCHECK_API const char* uvmcheck_file_name(const uvmcheck_design* d, uint32_t file);

UVMCHECK_API uint32_t uvmcheck_class_count(const uvmcheck_design* d);
/* Return 1 and fill *out, or 0 when an id is out of range. */
UVMCHECK_API int uvmcheck_get_class(const uvmcheck_design* d, uint32_t cls, uvmcheck_class* out);
UVMCHECK_API int uvmcheck_get_property(const uvmcheck_design* d, uint32_t cls, uint32_t index,
                                       uvmcheck_property* out);
UVMCHECK_API int uvmcheck_get_method(const uvmcheck_design* d, uint32_t cls, uint32_t index,
                                     uvmcheck_method* out);
UVMCHECK_API int uvmcheck_get_argument(const uvmcheck_design* d, uint32_t cls, uint32_t method,
                                       uint32_t index, uvmcheck_argument* out);
UVMCHECK_API int uvmcheck_get_local(const uvmcheck_design* d, uint32_t cls, uint32_t method,
                                    uint32_t index, uvmcheck_local* out);
UVMCHECK_API int uvmcheck_get_call(const uvmcheck_design* d, uint32_t cls, uint32_t method,
                                   uint32_t index, uvmcheck_call* out);
UVMCHECK_API int uvmcheck_get_assignment(const uvmcheck_design* d, uint32_t cls, uint32_t method,
                                         uint32_t index, uvmcheck_assignment* out);
/* Delays, event controls and waits in a method body, in walk order. */
UVMCHECK_API int uvmcheck_get_timing(const uvmcheck_design* d, uint32_t cls, uint32_t method,
                                     uint32_t index, uvmcheck_loc* out);
UVMCHECK_API int uvmcheck_get_constraint(const uvmcheck_design* d, uint32_t cls, uint32_t index,
                                         uvmcheck_constraint* out);
UVMCHECK_API int uvmcheck_get_macro(const uvmcheck_design* d, uint32_t cls, uint32_t index, uvmcheck_macro* out);
UVMCHECK_API int uvmcheck_get_typedef(const uvmcheck_design* d, uint32_t cls, uint32_t index,
                                      uvmcheck_typedef* out);

/* The whole model as JSON (schema uvmcheck.model/1), made on first call. */
UVMCHECK_API const char* uvmcheck_design_json(uvmcheck_design* d);

#ifdef __cplusplus
}
#endif

#endif
