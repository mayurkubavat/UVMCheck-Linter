#include "uvmcheck/model/SlangModel.h"

#include <unordered_map>

#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/Type.h"
#include "slang/text/SourceManager.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::model {

namespace {

namespace sa = slang::ast;

Visibility toVisibility(sa::Visibility v) {
    switch (v) {
        case sa::Visibility::Protected: return Visibility::Protected;
        case sa::Visibility::Local: return Visibility::Local;
        default: return Visibility::Public;
    }
}

UvmKind classify(const uvm::UvmModel& uvm, const sa::Type& t) {
    // Component and sequence checks come first: a reduced UVM (a test mock)
    // need not derive them from uvm_object.
    if (uvm.isUvmComponent(t)) {
        // Most specific first: a test is also a component.
        static constexpr std::pair<std::string_view, UvmKind> roles[] = {
            {"uvm_test", UvmKind::Test},
            {"uvm_env", UvmKind::Env},
            {"uvm_agent", UvmKind::Agent},
            {"uvm_driver", UvmKind::Driver},
            {"uvm_monitor", UvmKind::Monitor},
            {"uvm_sequencer_base", UvmKind::Sequencer},
            {"uvm_scoreboard", UvmKind::Scoreboard},
            {"uvm_subscriber", UvmKind::Subscriber},
        };
        for (const auto& [base, kind] : roles) {
            if (uvm.isDerivedFrom(t, base)) return kind;
        }
        return UvmKind::Component;
    }
    if (uvm.isUvmSequence(t)) return UvmKind::Sequence;
    if (uvm.isUvmSequenceItem(t)) return UvmKind::SequenceItem;
    return uvm.isUvmObject(t) ? UvmKind::Object : UvmKind::None;
}

class Builder {
public:
    Builder(const sa::Compilation& c, const uvm::UvmModel& u, const BuildOptions& o)
        : sm(c.getSourceManager()), uvm(u), options(o) {}

    Model run() {
        const auto& all = uvm.getAllClasses();
        // Pass 1: ids, so bases and property types can point at any class.
        for (const auto* ct : all) {
            if (ct) ids.try_emplace(ct, static_cast<ClassId>(ids.size()));
        }
        for (const auto* ct : all) {
            if (ct && ids.at(ct) == model.classes().size()) model.addClass(makeClass(*ct));
        }
        return std::move(model);
    }

private:
    const slang::SourceManager* sm;
    const uvm::UvmModel& uvm;
    const BuildOptions& options;
    Model model;
    std::unordered_map<const sa::ClassType*, ClassId> ids;

    Loc loc(slang::SourceLocation l) {
        Loc out;
        if (!l.valid()) return out;
        out.raw = (uint64_t(l.buffer().getId()) << 32) | uint64_t(l.offset() & 0xffffffffu);
        if (!sm) return out;
        // Code from a macro (`uvm_component_utils and the like) is reported
        // where the user wrote it, as findings are.
        const auto orig = sm->getFullyOriginalLoc(l);
        out.file = model.internFile(sm->getFileName(orig));
        out.line = static_cast<uint32_t>(sm->getLineNumber(orig));
        out.column = static_cast<uint32_t>(sm->getColumnNumber(orig));
        return out;
    }

    ClassId idOf(const sa::Type& t) const {
        const auto& canon = t.getCanonicalType();
        if (!canon.isClass()) return NoClass;
        auto it = ids.find(&canon.as<sa::ClassType>());
        return it == ids.end() ? NoClass : it->second;
    }

    Class makeClass(const sa::ClassType& ct) {
        Class c;
        c.name = std::string(ct.name);
        c.loc = loc(ct.location);
        if (const auto* parent = ct.getParentScope()) {
            const auto& sym = parent->asSymbol();
            if (sym.kind == sa::SymbolKind::Package || sym.kind == sa::SymbolKind::ClassType)
                c.scope = std::string(sym.name);
        }
        c.isVirtual = ct.isAbstract;
        c.isSpecialization = ct.genericClass != nullptr;
        if (const auto* base = ct.getBaseClass(); base && !base->isError()) {
            const auto& canon = base->getCanonicalType();
            if (canon.isClass()) {
                const auto& bct = canon.as<sa::ClassType>();
                c.baseName = std::string(bct.genericClass ? bct.genericClass->name : bct.name);
                c.base = idOf(canon);
            }
        }
        c.uvm = classify(uvm, ct);
        if (options.isLibraryFile) c.isLibrary = options.isLibraryFile(model.fileName(c.loc));

        for (const auto& member : ct.members()) {
            if (member.kind == sa::SymbolKind::ClassProperty) {
                const auto& p = member.as<sa::ClassPropertySymbol>();
                Property prop;
                prop.name = std::string(p.name);
                prop.type = p.getType().toString();
                prop.loc = loc(p.location);
                prop.visibility = toVisibility(p.visibility);
                prop.isStatic = p.lifetime == sa::VariableLifetime::Static;
                prop.isConst = p.flags.has(sa::VariableFlags::Const);
                prop.isRand = p.randMode != sa::RandMode::None;
                prop.typeClass = idOf(p.getType());
                c.properties.push_back(std::move(prop));
            } else if (member.kind == sa::SymbolKind::Subroutine) {
                const auto& s = member.as<sa::SubroutineSymbol>();
                Method m;
                m.name = std::string(s.name);
                m.loc = loc(s.location);
                m.visibility = toVisibility(s.visibility);
                m.isTask = s.subroutineKind == sa::SubroutineKind::Task;
                m.isVirtual = s.isVirtual();
                m.isPure = s.flags.has(sa::MethodFlags::Pure);
                m.isStatic = s.flags.has(sa::MethodFlags::Static);
                if (uvm::UvmModel::isFunctionPhase(s.name)) m.phase = PhaseKind::Function;
                else if (uvm::UvmModel::isTaskPhase(s.name)) m.phase = PhaseKind::Task;
                c.methods.push_back(std::move(m));
            }
        }
        return c;
    }
};

} // namespace

Model buildFromSlang(const slang::ast::Compilation& compilation, const uvm::UvmModel& uvm,
                     const BuildOptions& options) {
    return Builder(compilation, uvm, options).run();
}

slang::SourceLocation toSlang(const Loc& loc) {
    if (loc.raw == 0) return slang::SourceLocation::NoLocation;
    return slang::SourceLocation(slang::BufferID(static_cast<uint32_t>(loc.raw >> 32), {}),
                                 loc.raw & 0xffffffffu);
}

} // namespace uvmcheck::model
