#include "uvmcheck/compat/Overlay.h"

#include "uvmcheck/rules/CompilerDiagnostics.h"

#include "slang/ast/Compilation.h"
#include "slang/ast/Scope.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/diagnostics/Diagnostics.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::compat {

namespace {

using namespace slang;
using namespace slang::ast;

const SourceManager* sourceManager = nullptr;

/// The class specialization (e.g. uvm_component_registry#(my_comp, "my_comp"))
/// the symbol sits in, if any.
const ClassType* enclosingSpecialization(const Symbol* sym) {
    while (sym) {
        if (sym->kind == SymbolKind::ClassType) {
            auto& cls = sym->as<ClassType>();
            if (cls.genericClass) return &cls;
        }
        auto* scope = sym->getParentScope();
        sym = scope ? &scope->asSymbol() : nullptr;
    }
    return nullptr;
}

/// For an error inside a library template, the user code that instantiated it:
/// the typedef of that specialization inside the class passed as a type
/// parameter (where `uvm_component_utils is invoked), else that class itself.
/// For an error inside a macro expansion, the line that invoked the macro.
std::optional<SourceLocation> anchorToUserCode(const Diagnostic& diag) {
    if (!sourceManager || !diag.location.valid()) return std::nullopt;
    const auto& sm = *sourceManager;
    const ClassType* spec = enclosingSpecialization(diag.symbol);
    if (!spec) {
        if (sm.isMacroLoc(diag.location)) return sm.getFullyExpandedLoc(diag.location);
        return std::nullopt;
    }
    const auto diagBuffer = sm.getFullyOriginalLoc(diag.location).buffer();

    for (const auto& member : spec->members()) {
        if (member.kind != SymbolKind::TypeParameter) continue;
        const auto& target = member.as<TypeParameterSymbol>().targetType.getType().getCanonicalType();
        if (target.kind != SymbolKind::ClassType) continue;
        const auto& userClass = target.as<ClassType>();
        auto classLoc = sm.getFullyExpandedLoc(userClass.location);
        if (!classLoc.valid() || classLoc.buffer() == diagBuffer) continue;

        for (const auto& m : userClass.members()) {
            if (m.kind != SymbolKind::TypeAlias) continue;
            if (&m.as<TypeAliasType>().targetType.getType().getCanonicalType() == spec)
                return sm.getFullyExpandedLoc(m.location);
        }
        return classLoc;
    }
    return std::nullopt;
}

/// Errors slang raises while elaborating a library template for a user class,
/// such as a wrong constructor signature reported at uvm_registry.svh:201, are
/// moved to the user's `uvm_*_utils line (or class), with the library line kept
/// as a note; so are errors inside a library macro's expansion. Without this they
/// point into UVM, and with UVM as a library path (or report_library_macro_expansions
/// off) they are dropped with the rest of its findings, hiding a real compile error.
class ErrorAnchor : public Override {
public:
    std::string_view id() const override { return "anchor-to-user-code"; }
    std::string_view summary() const override {
        return "Report compile errors raised inside a library template (uvm_registry.svh, ...) at the user class "
               "or `uvm_*_utils line that instantiated it";
    }
    // Without it a testbench that does not compile can lint clean, so it is not opt-in.
    bool onByDefault() const override { return true; }

    void configureCompilation(Compilation& compilation) override {
        sourceManager = compilation.getSourceManager();
        rules::setCompilerErrorAnchor(&anchorToUserCode);
    }
};

} // namespace

std::unique_ptr<Override> createErrorAnchor() {
    return std::make_unique<ErrorAnchor>();
}

} // namespace uvmcheck::compat
