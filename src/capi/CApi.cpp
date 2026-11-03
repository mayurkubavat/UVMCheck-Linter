#include "uvmcheck/model/CApi.h"

#include <sstream>
#include <string>
#include <vector>

#include "uvmcheck/model/Session.h"

using namespace uvmcheck::model;

struct uvmcheck_design {
    std::unique_ptr<Session> session;
    std::string error;
    std::string json;
};

namespace {

uvmcheck_loc toC(const Loc& l) { return {l.file, l.line, l.column}; }

const Model* modelOf(const uvmcheck_design* d) {
    return d && d->session && d->session->ok() ? &d->session->model() : nullptr;
}

} // namespace

extern "C" {

uint32_t uvmcheck_model_abi_version(void) { return UVMCHECK_MODEL_ABI_VERSION; }

uvmcheck_design* uvmcheck_design_load(int argc, const char* const* argv) {
    auto* d = new uvmcheck_design();
    try {
        std::vector<std::string> args;
        for (int i = 0; i < argc; ++i) args.emplace_back(argv[i] ? argv[i] : "");
        d->session = Session::load(args);
        if (!d->session->ok()) d->error = d->session->error();
    } catch (const std::exception& e) {
        d->error = e.what();
    } catch (...) {
        d->error = "unknown error";
    }
    return d;
}

void uvmcheck_design_free(uvmcheck_design* d) { delete d; }

const char* uvmcheck_design_error(const uvmcheck_design* d) {
    if (!d) return "null design";
    return d->error.empty() ? nullptr : d->error.c_str();
}

uint32_t uvmcheck_design_compile_errors(const uvmcheck_design* d) {
    return modelOf(d) ? static_cast<uint32_t>(d->session->errorCount()) : 0;
}

double uvmcheck_design_time_ms(const uvmcheck_design* d, int stage) {
    if (!modelOf(d)) return 0;
    const auto& t = d->session->timings();
    switch (stage) {
        case 0: return t.parseMs;
        case 1: return t.elaborateMs;
        case 2: return t.modelMs;
        default: return 0;
    }
}

uint32_t uvmcheck_file_count(const uvmcheck_design* d) {
    const Model* m = modelOf(d);
    return m ? static_cast<uint32_t>(m->files().size()) : 0;
}

const char* uvmcheck_file_name(const uvmcheck_design* d, uint32_t file) {
    const Model* m = modelOf(d);
    if (!m || file >= m->files().size()) return nullptr;
    return m->files()[file].c_str();
}

uint32_t uvmcheck_class_count(const uvmcheck_design* d) {
    const Model* m = modelOf(d);
    return m ? static_cast<uint32_t>(m->classes().size()) : 0;
}

int uvmcheck_get_class(const uvmcheck_design* d, uint32_t cls, uvmcheck_class* out) {
    const Model* m = modelOf(d);
    if (!m || !out || cls >= m->classes().size()) return 0;
    const Class& c = m->classes()[cls];
    *out = {c.name.c_str(), c.scope.c_str(), c.baseName.c_str(), c.base, static_cast<uint8_t>(c.uvm),
            c.isVirtual, c.isParameterized, c.isLibrary, toC(c.loc),
            static_cast<uint32_t>(c.properties.size()), static_cast<uint32_t>(c.methods.size())};
    return 1;
}

int uvmcheck_get_property(const uvmcheck_design* d, uint32_t cls, uint32_t index, uvmcheck_property* out) {
    const Model* m = modelOf(d);
    if (!m || !out || cls >= m->classes().size()) return 0;
    const auto& props = m->classes()[cls].properties;
    if (index >= props.size()) return 0;
    const Property& p = props[index];
    *out = {p.name.c_str(), p.type.c_str(), p.typeClass, static_cast<uint8_t>(p.visibility),
            p.isStatic, p.isConst, p.isRand, toC(p.loc)};
    return 1;
}

int uvmcheck_get_method(const uvmcheck_design* d, uint32_t cls, uint32_t index, uvmcheck_method* out) {
    const Model* m = modelOf(d);
    if (!m || !out || cls >= m->classes().size()) return 0;
    const auto& methods = m->classes()[cls].methods;
    if (index >= methods.size()) return 0;
    const Method& x = methods[index];
    *out = {x.name.c_str(), static_cast<uint8_t>(x.visibility), x.isTask, x.isVirtual, x.isPure,
            x.isStatic, static_cast<uint8_t>(x.phase), toC(x.loc)};
    return 1;
}

const char* uvmcheck_design_json(uvmcheck_design* d) {
    const Model* m = modelOf(d);
    if (!m) return nullptr;
    if (d->json.empty()) {
        std::ostringstream out;
        m->writeJson(out);
        d->json = out.str();
    }
    return d->json.c_str();
}

} // extern "C"
