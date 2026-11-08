// Rules driven by the UVM API pack of the UVM version being compiled
// (uvm::ApiPack, data/uvm-api/). They hold no API names of their own: a new
// UVM release only needs its pack.
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/ApiPack.h"
#include "../methodology/MethodologySupport.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/syntax/AllSyntax.h"
#include <set>
#include <string>

namespace uvmcheck::rules::uvmapi {

using namespace slang::ast;
using slang::parsing::Token;
using slang::parsing::TriviaKind;
using slang::syntax::SyntaxKind;

namespace {

/// The class a subroutine is a member of, by its generic name for a specialization.
std::string_view owningClassName(const SubroutineSymbol& sub) {
    const Scope* scope = sub.getParentScope();
    if (!scope) return {};
    const Symbol& sym = scope->asSymbol();
    if (sym.kind != SymbolKind::ClassType) return {};
    const auto& cls = sym.as<ClassType>();
    if (cls.genericClass) return cls.genericClass->name;
    return cls.name;
}

bool inLibraryCode(const Compilation& comp, slang::SourceLocation loc) {
    const auto* sm = comp.getSourceManager();
    if (!sm || !loc.valid()) return true;
    // A call a library macro expands to is the macro's, not the user's.
    if (sm->isMacroLoc(loc)) return true;
    auto orig = sm->getFullyOriginalLoc(loc);
    return !orig.valid() || meth::isLibraryBuffer(*sm, orig.buffer());
}

} // namespace

// --- UVM-API-001: an API of the Accellera library that IEEE 1800.2 does not define -------

class AccelleraOnlyApiVisitor : public engine::AstVisitor<AccelleraOnlyApiVisitor> {
public:
    AccelleraOnlyApiVisitor(const Compilation& comp, const uvm::ApiPack& pack, core::DiagnosticEngine& diags)
        : comp(comp), pack(pack), diags(diags) {}

    void handle(const CallExpression& call) {
        if (!call.isSystemCall()) {
            if (const auto* sub = std::get_if<0>(&call.subroutine); sub && *sub) {
                check((*sub)->name, owningClassName(**sub), call.sourceRange.start());
            }
        }
        visitDefault(call);
    }

    void handle(const ClassType& cls) {
        if (const auto* base = cls.getBaseClass(); base && !inLibraryCode(comp, cls.location)) {
            const Type& canon = base->getCanonicalType();
            if (canon.kind == SymbolKind::ClassType) {
                const auto& bc = canon.as<ClassType>();
                std::string_view name = bc.genericClass ? bc.genericClass->name : bc.name;
                if (const auto* e = pack.findClass(name); e && e->tag == "accellera") {
                    report("class '" + std::string(name) + "'", "'" + std::string(cls.name) + "' extends it",
                           cls.location);
                }
            }
        }
        visitDefault(cls);
    }

private:
    void check(std::string_view member, std::string_view cls, slang::SourceLocation loc) {
        if (cls.empty() || !cls.starts_with("uvm_") || inLibraryCode(comp, loc)) return;
        const auto* e = pack.findMember(cls, member);
        if (!e || e->tag != "accellera") return;
        if (!seen.insert(loc).second) return;
        report("'" + std::string(cls) + "::" + std::string(member) + "'", "called here", loc);
    }

    void report(const std::string& what, const std::string& where, slang::SourceLocation loc) {
        diags.addViolation(
            "UVM-API-001", "AccelleraOnlyApi",
            what + " (" + where + ") is a detail of the Accellera UVM library (tagged @uvm-accellera in UVM " +
                pack.version() + "), not part of IEEE 1800.2. Another UVM implementation, or a later Accellera "
                "release, need not have it. Keep such calls in one debug helper, or use the standard API.",
            core::Severity::Info, loc);
    }

    const Compilation& comp;
    const uvm::ApiPack& pack;
    core::DiagnosticEngine& diags;
    std::set<slang::SourceLocation> seen;
};

class AccelleraOnlyApi : public Rule {
public:
    std::string_view getId() const override { return "UVM-API-001"; }
    std::string_view getName() const override { return "AccelleraOnlyApi"; }
    std::string_view getDescription() const override {
        return "Flags calls to, and classes extending, parts of the Accellera UVM library that IEEE 1800.2 does "
               "not define (tagged @uvm-accellera in the library source), such as uvm_component::print_config "
               "or uvm_port_base::debug_connected_to. Code that relies on them is tied to one UVM "
               "implementation and release. The list comes from the UVM API pack of the version being compiled.";
    }
    std::string_view getCategory() const override { return "Portability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        const auto* pack = uvm::ApiPack::forCompilation(ctx.compilation());
        if (!pack) co_return;
        AccelleraOnlyApiVisitor visitor(ctx.compilation(), *pack, ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

// --- UVM-API-002: a macro the UVM version compiles only as deprecated API ---------------

class DeprecatedMacroVisitor : public engine::SyntaxVisitor<DeprecatedMacroVisitor> {
public:
    DeprecatedMacroVisitor(const slang::SourceManager& sm, const uvm::ApiPack& pack, core::DiagnosticEngine& diags)
        : sm(sm), pack(pack), diags(diags) {}

    void visitToken(Token token) {
        for (const auto& trivia : token.trivia()) {
            if (trivia.kind != TriviaKind::Directive) continue;
            const auto* d = trivia.syntax();
            if (!d || d->kind != SyntaxKind::MacroUsage) continue;
            const auto& usage = d->as<slang::syntax::MacroUsageSyntax>();
            std::string_view name = usage.directive.valueText();
            if (name.starts_with("`")) name.remove_prefix(1);
            if (!pack.isDeprecatedMacro(name)) continue;
            auto loc = usage.directive.location();
            if (!loc.valid() || !sm.isFileLoc(loc) || meth::isLibraryBuffer(sm, loc.buffer())) continue;
            if (!seen.insert(loc).second) continue;
            diags.addViolation(
                "UVM-API-002", "DeprecatedUvmMacro",
                "`" + std::string(name) + " is deprecated in UVM " + pack.version() +
                    ": the library defines it only when UVM_ENABLE_DEPRECATED_API is set, so this line stops "
                    "compiling without that define. For the `uvm_do family use `uvm_do(SEQ_OR_ITEM, SEQR, PRIORITY, "
                    "{CONSTRAINTS}) and its `uvm_send / `uvm_rand_send forms with the extra arguments.",
                core::Severity::Warning, loc);
        }
    }

private:
    const slang::SourceManager& sm;
    const uvm::ApiPack& pack;
    core::DiagnosticEngine& diags;
    std::set<slang::SourceLocation> seen;
};

class DeprecatedUvmMacro : public Rule {
public:
    std::string_view getId() const override { return "UVM-API-002"; }
    std::string_view getName() const override { return "DeprecatedUvmMacro"; }
    std::string_view getDescription() const override {
        return "Flags uses of UVM macros that the UVM version being compiled defines only under "
               "UVM_ENABLE_DEPRECATED_API (in UVM 2020: `uvm_do_with, `uvm_do_on, `uvm_do_pri, `uvm_create_on, "
               "`uvm_send_pri and the rest of that family). Without the define the code does not compile. The "
               "list comes from the UVM API pack of the version being compiled.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        const auto* pack = uvm::ApiPack::forCompilation(ctx.compilation());
        const auto* sm = ctx.compilation().getSourceManager();
        if (!pack || !sm) co_return;
        DeprecatedMacroVisitor visitor(*sm, *pack, ctx.diags());
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvmapi

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAccelleraOnlyApi() {
    return std::make_unique<uvmapi::AccelleraOnlyApi>();
}
std::unique_ptr<Rule> createDeprecatedUvmMacro() {
    return std::make_unique<uvmapi::DeprecatedUvmMacro>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createAccelleraOnlyApi)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createDeprecatedUvmMacro)
