/* Compiled as C: proves libuvmcheck_model's ABI is usable without C++.
 * Loads SV-OOP-007's bad example and checks what the model says about it. */
#include <stdio.h>
#include <string.h>

#include "uvmcheck/model/CApi.h"

#define CHECK(cond)                                                       \
    do {                                                                  \
        if (!(cond)) {                                                    \
            fprintf(stderr, "capi_smoke: %s:%d: %s\n", __FILE__, __LINE__, #cond); \
            return 1;                                                     \
        }                                                                 \
    } while (0)

int main(int argc, char** argv) {
    CHECK(argc == 4); /* <tests/common dir> <bad.sv> <facts.sv> */
    CHECK(uvmcheck_model_abi_version() == UVMCHECK_MODEL_ABI_VERSION);
    /* rust/sv-slang asserts the same sizes for its copies of these structs */
    if (sizeof(void*) == 8) {
        CHECK(sizeof(uvmcheck_class) == 80 && sizeof(uvmcheck_property) == 48);
        CHECK(sizeof(uvmcheck_method) == 104 && sizeof(uvmcheck_argument) == 56);
        CHECK(sizeof(uvmcheck_assignment) == 48);
        CHECK(sizeof(uvmcheck_macro) == 32 && sizeof(uvmcheck_typedef) == 32);
        CHECK(sizeof(uvmcheck_call) == 72);
        CHECK(sizeof(uvmcheck_local) == 40 && sizeof(uvmcheck_constraint) == 32);
    }

    char incdir[4096];
    snprintf(incdir, sizeof incdir, "+incdir+%s", argv[1]);
    const char* args[] = {incdir, argv[2]};
    uvmcheck_design* d = uvmcheck_design_load(2, args);
    CHECK(d != NULL);
    CHECK(uvmcheck_design_error(d) == NULL);
    CHECK(uvmcheck_design_compile_errors(d) == 0);
    CHECK(uvmcheck_class_count(d) > 10);

    uint32_t found = UVMCHECK_NONE;
    for (uint32_t i = 0; i < uvmcheck_class_count(d); ++i) {
        uvmcheck_class c;
        CHECK(uvmcheck_get_class(d, i, &c));
        if (strcmp(c.name, "MultiChannelMonitor") == 0) found = i;
    }
    CHECK(found != UVMCHECK_NONE);

    uvmcheck_class c;
    CHECK(uvmcheck_get_class(d, found, &c));
    CHECK(c.uvm == UVMCHECK_UVM_COMPONENT);
    CHECK(strcmp(c.base_name, "uvm_component") == 0);
    CHECK(c.base != UVMCHECK_NONE);
    CHECK(strstr(uvmcheck_file_name(d, c.loc.file), "bad.sv") != NULL);

    int sawStatic = 0;
    for (uint32_t i = 0; i < c.property_count; ++i) {
        uvmcheck_property p;
        CHECK(uvmcheck_get_property(d, found, i, &p));
        if (strcmp(p.name, "m_global_packet_counter") == 0) {
            sawStatic = p.is_static && !p.is_const && p.loc.line == 10;
        }
    }
    CHECK(sawStatic);

    int sawNew = 0;
    for (uint32_t i = 0; i < c.method_count; ++i) {
        uvmcheck_method m;
        CHECK(uvmcheck_get_method(d, found, i, &m));
        if (strcmp(m.name, "new") == 0) sawNew = 1;
    }
    CHECK(sawNew);

    /* Out of range and bad input are refused, not crashes, and nothing is printed. */
    CHECK(!uvmcheck_get_class(d, UVMCHECK_NONE, &c));
    CHECK(uvmcheck_file_name(d, UVMCHECK_NONE) == NULL);
    const char* json = uvmcheck_design_json(d);
    CHECK(json && strstr(json, "\"schema\":\"uvmcheck.model/1\"") && strstr(json, "MultiChannelMonitor"));
    uvmcheck_design_free(d);

    /* Arguments, locals, constraints and extern methods (ABI v3). */
    const char* factArgs[] = {incdir, argv[3]};
    d = uvmcheck_design_load(2, factArgs);
    CHECK(uvmcheck_design_error(d) == NULL && uvmcheck_design_compile_errors(d) == 0);
    found = UVMCHECK_NONE;
    for (uint32_t i = 0; i < uvmcheck_class_count(d); ++i) {
        CHECK(uvmcheck_get_class(d, i, &c));
        if (strcmp(c.name, "facts_item") == 0) found = i;
    }
    CHECK(found != UVMCHECK_NONE);
    CHECK(uvmcheck_get_class(d, found, &c));
    CHECK(c.uvm == UVMCHECK_UVM_SEQUENCE_ITEM);
    CHECK(c.constraint_count == 1);
    uvmcheck_constraint k;
    CHECK(uvmcheck_get_constraint(d, found, 0, &k));
    CHECK(strcmp(k.name, "addr_c") == 0 && k.loc.line == 11);
    CHECK(!uvmcheck_get_constraint(d, found, 1, &k));

    int sawSum = 0, sawProbe = 0;
    for (uint32_t i = 0; i < c.method_count; ++i) {
        uvmcheck_method m;
        CHECK(uvmcheck_get_method(d, found, i, &m));
        if (strcmp(m.name, "sum") == 0) {
            /* extern: the record points at the body after the class */
            CHECK(m.is_extern && m.declared_virtual && m.is_virtual && !m.returns_void);
            CHECK(strcmp(m.return_type, "int") == 0 && m.loc.line == 22);
            CHECK(m.argument_count == 2 && m.local_count == 1);
            uvmcheck_argument a;
            CHECK(uvmcheck_get_argument(d, found, i, 0, &a));
            CHECK(strcmp(a.name, "q") == 0 && a.direction == UVMCHECK_DIR_REF);
            CHECK(a.shape == UVMCHECK_SHAPE_QUEUE && a.type_class == UVMCHECK_NONE);
            CHECK(uvmcheck_get_argument(d, found, i, 1, &a));
            CHECK(a.direction == UVMCHECK_DIR_IN && a.shape == UVMCHECK_SHAPE_OTHER);
            CHECK(!uvmcheck_get_argument(d, found, i, 2, &a));
            uvmcheck_local l;
            CHECK(uvmcheck_get_local(d, found, i, 0, &l));
            CHECK(strcmp(l.name, "total") == 0 && !l.is_virtual_interface);
            sawSum = 1;
        } else if (strcmp(m.name, "probe") == 0) {
            CHECK(m.is_extern && m.returns_void && !m.is_virtual && m.local_count == 1);
            uvmcheck_local l;
            CHECK(uvmcheck_get_local(d, found, i, 0, &l));
            CHECK(strcmp(l.name, "vif") == 0 && l.is_virtual_interface);
            /* randomize, set, and $urandom nested in set's arguments */
            CHECK(m.call_count == 3);
            uvmcheck_call k0, k1, k2;
            CHECK(uvmcheck_get_call(d, found, i, 0, &k0) && uvmcheck_get_call(d, found, i, 1, &k1) &&
                  uvmcheck_get_call(d, found, i, 2, &k2) && !uvmcheck_get_call(d, found, i, 3, &k2));
            CHECK(strcmp(k0.name, "randomize") == 0 && k0.is_system); /* built-in method */
            CHECK(strcmp(k1.name, "set") == 0 && strcmp(k1.callee_scope, "uvm_config_db") == 0);
            CHECK(k1.first_arg_is_null && k1.loc.line == 31 && strcmp(k1.receiver, "") == 0);
            CHECK(strcmp(k2.name, "$urandom") == 0 && k2.is_system && strcmp(k2.callee_scope, "") == 0);
            sawProbe = 1;
        } else if (strcmp(m.name, "new") == 0) {
            CHECK(!m.is_extern && !m.is_builtin);
        } else if (strcmp(m.name, "post_randomize") == 0) {
            uvmcheck_call k;
            CHECK(m.call_count == 1 && uvmcheck_get_call(d, found, i, 0, &k));
            CHECK(k.via_super && strcmp(k.name, "post_randomize") == 0 && strcmp(k.receiver, "super") == 0);
        } else if (strcmp(m.name, "randomize") == 0) {
            CHECK(m.is_builtin);
        }
    }
    CHECK(sawSum && sawProbe);

    /* Assignments, timing controls and super.new (ABI v5). */
    uint32_t sub = UVMCHECK_NONE;
    for (uint32_t i = 0; i < uvmcheck_class_count(d); ++i) {
        CHECK(uvmcheck_get_class(d, i, &c));
        if (strcmp(c.name, "facts_sub") == 0) sub = i;
    }
    CHECK(sub != UVMCHECK_NONE && uvmcheck_get_class(d, sub, &c));
    int sawCtor = 0, sawWrite = 0, sawWait = 0;
    for (uint32_t i = 0; i < c.method_count; ++i) {
        uvmcheck_method m;
        CHECK(uvmcheck_get_method(d, sub, i, &m));
        if (strcmp(m.name, "new") == 0) {
            CHECK(m.calls_super_new && m.super_new_index == 0 && m.super_new_loc.line == 37);
            sawCtor = 1;
        } else if (strcmp(m.name, "write") == 0) {
            uvmcheck_assignment a;
            CHECK(m.assignment_count == 2 && uvmcheck_get_assignment(d, sub, i, 0, &a));
            CHECK(strcmp(a.lhs, "t.addr") == 0 && strcmp(a.rhs, "0") == 0 && a.is_non_blocking);
            CHECK(a.root_kind == UVMCHECK_ROOT_ARGUMENT && strcmp(a.root_name, "t") == 0);
            CHECK(uvmcheck_get_assignment(d, sub, i, 1, &a));
            CHECK(strcmp(a.lhs, "count") == 0 && strcmp(a.rhs, "t.addr") == 0 && !a.is_non_blocking);
            CHECK(a.root_kind == UVMCHECK_ROOT_PROPERTY && !uvmcheck_get_assignment(d, sub, i, 2, &a));
            CHECK(!m.calls_super_new && m.super_new_index == -1);
            sawWrite = 1;
        } else if (strcmp(m.name, "wait_a_bit") == 0) {
            uvmcheck_loc t;
            CHECK(m.timing_count == 1 && uvmcheck_get_timing(d, sub, i, 0, &t) && t.line == 45);
            sawWait = 1;
        }
    }
    CHECK(sawCtor && sawWrite && sawWait);
    CHECK(strstr(uvmcheck_design_json(d), "\"extern\":true") != NULL);
    uvmcheck_design_free(d);

    const char* missing[] = {"/no/such/file.sv"};
    d = uvmcheck_design_load(1, missing);
    CHECK(uvmcheck_design_error(d) != NULL);
    CHECK(strstr(uvmcheck_design_error(d), "/no/such/file.sv") != NULL);
    CHECK(uvmcheck_class_count(d) == 0);
    uvmcheck_design_free(d);

    printf("capi_smoke: ok\n");
    return 0;
}
