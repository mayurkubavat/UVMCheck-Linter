#include "uvmcheck/model/Model.h"

namespace uvmcheck::model {

std::string_view toString(UvmKind kind) {
    switch (kind) {
        case UvmKind::None: return "none";
        case UvmKind::Object: return "object";
        case UvmKind::SequenceItem: return "sequence_item";
        case UvmKind::Sequence: return "sequence";
        case UvmKind::Component: return "component";
        case UvmKind::Test: return "test";
        case UvmKind::Env: return "env";
        case UvmKind::Agent: return "agent";
        case UvmKind::Driver: return "driver";
        case UvmKind::Monitor: return "monitor";
        case UvmKind::Sequencer: return "sequencer";
        case UvmKind::Scoreboard: return "scoreboard";
        case UvmKind::Subscriber: return "subscriber";
    }
    return "none";
}

const Property* Class::findProperty(std::string_view n) const {
    for (const auto& p : properties)
        if (p.name == n) return &p;
    return nullptr;
}

size_t Method::countCalls(std::string_view n) const {
    size_t k = 0;
    for (const auto& c : calls) k += c.name == n;
    return k;
}

const Method* Class::findMethod(std::string_view n) const {
    for (const auto& m : methods)
        if (m.name == n) return &m;
    return nullptr;
}

std::optional<ClassId> Model::findClass(std::string_view name) const {
    auto it = byName_.find(std::string(name));
    if (it == byName_.end()) return std::nullopt;
    return it->second;
}

std::vector<ClassId> Model::ancestors(ClassId id) const {
    std::vector<ClassId> out;
    for (ClassId b = classes_.at(id).base; b != NoClass && out.size() < classes_.size(); b = classes_.at(b).base) {
        out.push_back(b);
    }
    return out;
}

bool Model::derivesFrom(ClassId id, std::string_view ancestorName) const {
    // Bounded walk: a malformed (cyclic) chain cannot hang a reader.
    for (size_t steps = 0; id != NoClass && steps < classes_.size() + 1; ++steps) {
        const Class& c = classes_.at(id);
        if (c.name == ancestorName) return true;
        if (c.base == NoClass) return c.baseName == ancestorName;
        id = c.base;
    }
    return false;
}

ClassId Model::addClass(Class c) {
    const auto id = static_cast<ClassId>(classes_.size());
    byName_.try_emplace(c.name, id);
    classes_.push_back(std::move(c));
    return id;
}

uint32_t Model::internFile(std::string_view name) {
    if (name.empty()) return 0;
    auto [it, inserted] = fileIds_.try_emplace(std::string(name), static_cast<uint32_t>(files_.size()));
    if (inserted) files_.emplace_back(name);
    return it->second;
}

namespace {

void writeString(std::ostream& out, std::string_view s) {
    out << '"';
    for (char ch : s) {
        switch (ch) {
            case '"': out << "\\\""; break;
            case '\\': out << "\\\\"; break;
            case '\n': out << "\\n"; break;
            case '\r': out << "\\r"; break;
            case '\t': out << "\\t"; break;
            default:
                if (static_cast<unsigned char>(ch) < 0x20) {
                    static const char* hex = "0123456789abcdef";
                    out << "\\u00" << hex[(ch >> 4) & 0xF] << hex[ch & 0xF];
                } else {
                    out << ch;
                }
        }
    }
    out << '"';
}

const char* visibilityName(Visibility v) {
    switch (v) {
        case Visibility::Public: return "public";
        case Visibility::Protected: return "protected";
        case Visibility::Local: return "local";
    }
    return "public";
}

const char* directionName(Direction d) {
    switch (d) {
        case Direction::In: return "in";
        case Direction::Out: return "out";
        case Direction::InOut: return "inout";
        case Direction::Ref: return "ref";
    }
    return "in";
}

const char* shapeName(TypeShape t) {
    switch (t) {
        case TypeShape::Other: return "other";
        case TypeShape::DynamicArray: return "dynamic_array";
        case TypeShape::Queue: return "queue";
        case TypeShape::AssociativeArray: return "associative_array";
        case TypeShape::FixedUnpackedArray: return "fixed_unpacked_array";
        case TypeShape::UnpackedStruct: return "unpacked_struct";
    }
    return "other";
}

const char* phaseName(PhaseKind p) {
    switch (p) {
        case PhaseKind::None: return "none";
        case PhaseKind::Function: return "function";
        case PhaseKind::Task: return "task";
    }
    return "none";
}

void writeLoc(std::ostream& out, const Loc& loc) {
    out << "\"file\":" << loc.file << ",\"line\":" << loc.line << ",\"column\":" << loc.column;
}

} // namespace

void Model::writeJson(std::ostream& out) const {
    out << "{\"schema\":\"uvmcheck.model/1\",\"files\":[";
    for (size_t i = 0; i < files_.size(); ++i) {
        if (i) out << ',';
        writeString(out, files_[i]);
    }
    out << "],\"classes\":[";
    for (size_t i = 0; i < classes_.size(); ++i) {
        const Class& c = classes_[i];
        if (i) out << ',';
        out << "\n{\"id\":" << i << ",\"name\":";
        writeString(out, c.name);
        out << ",\"scope\":";
        writeString(out, c.scope);
        out << ",\"base_name\":";
        writeString(out, c.baseName);
        out << ",\"base\":";
        if (c.base == NoClass) out << "null"; else out << c.base;
        out << ",\"uvm\":\"" << toString(c.uvm) << "\",\"virtual\":" << (c.isVirtual ? "true" : "false")
            << ",\"parameterized\":" << (c.isParameterized ? "true" : "false")
            << ",\"library\":" << (c.isLibrary ? "true" : "false") << ',';
        writeLoc(out, c.loc);
        out << ",\"properties\":[";
        for (size_t j = 0; j < c.properties.size(); ++j) {
            const Property& p = c.properties[j];
            if (j) out << ',';
            out << "{\"name\":";
            writeString(out, p.name);
            out << ",\"type\":";
            writeString(out, p.type);
            out << ",\"visibility\":\"" << visibilityName(p.visibility) << "\",\"static\":"
                << (p.isStatic ? "true" : "false") << ",\"const\":" << (p.isConst ? "true" : "false")
                << ",\"rand\":" << (p.isRand ? "true" : "false")
                << ",\"virtual_interface\":" << (p.isVirtualInterface ? "true" : "false") << ",\"type_class\":";
            if (p.typeClass == NoClass) out << "null"; else out << p.typeClass;
            out << ',';
            writeLoc(out, p.loc);
            out << '}';
        }
        out << "],\"methods\":[";
        for (size_t j = 0; j < c.methods.size(); ++j) {
            const Method& m = c.methods[j];
            if (j) out << ',';
            out << "{\"name\":";
            writeString(out, m.name);
            out << ",\"visibility\":\"" << visibilityName(m.visibility) << "\",\"task\":"
                << (m.isTask ? "true" : "false") << ",\"virtual\":" << (m.isVirtual ? "true" : "false")
                << ",\"pure\":" << (m.isPure ? "true" : "false") << ",\"static\":"
                << (m.isStatic ? "true" : "false") << ",\"declared_virtual\":"
                << (m.declaredVirtual ? "true" : "false") << ",\"builtin\":" << (m.isBuiltIn ? "true" : "false")
                << ",\"extern\":" << (m.isExtern ? "true" : "false")
                << ",\"phase\":\"" << phaseName(m.phase) << "\",\"return_type\":";
            writeString(out, m.returnType);
            out << ",\"arguments\":[";
            for (size_t k = 0; k < m.arguments.size(); ++k) {
                const Argument& a = m.arguments[k];
                if (k) out << ',';
                out << "{\"name\":";
                writeString(out, a.name);
                out << ",\"type\":";
                writeString(out, a.type);
                out << ",\"canonical_type\":";
                writeString(out, a.canonicalType);
                out << ",\"direction\":\"" << directionName(a.direction) << "\",\"shape\":\"" << shapeName(a.shape)
                    << "\",\"type_class\":";
                if (a.typeClass == NoClass) out << "null"; else out << a.typeClass;
                out << ',';
                writeLoc(out, a.loc);
                out << '}';
            }
            out << "],\"locals\":[";
            for (size_t k = 0; k < m.locals.size(); ++k) {
                const Local& l = m.locals[k];
                if (k) out << ',';
                out << "{\"name\":";
                writeString(out, l.name);
                out << ",\"type\":";
                writeString(out, l.type);
                out << ",\"virtual_interface\":" << (l.isVirtualInterface ? "true" : "false") << ',';
                writeLoc(out, l.loc);
                out << '}';
            }
            out << "],\"calls\":[";
            for (size_t k = 0; k < m.calls.size(); ++k) {
                const Call& x = m.calls[k];
                if (k) out << ',';
                out << "{\"name\":";
                writeString(out, x.name);
                out << ",\"system\":" << (x.isSystem ? "true" : "false") << ",\"callee_scope\":";
                writeString(out, x.calleeScope);
                out << ",\"callee_declared_virtual\":" << (x.calleeDeclaredVirtual ? "true" : "false")
                    << ",\"callee_builtin\":" << (x.calleeBuiltIn ? "true" : "false")
                    << ",\"first_arg_null\":" << (x.firstArgIsNull ? "true" : "false")
                    << ",\"via_super\":" << (x.viaSuper ? "true" : "false") << ",\"receiver\":";
                writeString(out, x.receiver);
                out << ',';
                writeLoc(out, x.loc);
                out << '}';
            }
            out << "],";
            writeLoc(out, m.loc);
            out << '}';
        }
        out << "],\"constraints\":[";
        for (size_t j = 0; j < c.constraints.size(); ++j) {
            if (j) out << ',';
            out << "{\"name\":";
            writeString(out, c.constraints[j].name);
            out << ',';
            writeLoc(out, c.constraints[j].loc);
            out << '}';
        }
        out << "]}";
    }
    out << "\n]}\n";
}

} // namespace uvmcheck::model
