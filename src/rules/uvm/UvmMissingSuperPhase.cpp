#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules::uvm {

class SuperCallCheckVisitor : public slang::ast::ASTVisitor<SuperCallCheckVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit SuperCallCheckVisitor(std::string_view phaseName)
        : targetPhase(phaseName) {}

    void handle(const slang::ast::CallExpression& expr) {
        if (expr.getSubroutineName() == targetPhase) {
            hasSuperCall = true;
        }
        visitDefault(expr);
    }

    bool foundSuperCall() const { return hasSuperCall; }

private:
    std::string_view targetPhase;
    bool hasSuperCall = false;
};

namespace {

/// Leading whitespace of the line holding `loc`.
std::string lineIndent(const slang::SourceManager& sm, slang::SourceLocation loc) {
    std::string_view text = sm.getSourceText(loc.buffer());
    size_t start = loc.offset();
    while (start > 0 && text[start - 1] != '\n') --start;
    size_t end = start;
    while (end < text.size() && (text[end] == ' ' || text[end] == '\t')) ++end;
    return std::string(text.substr(start, end - start));
}

/// Inserts `super.<phase>(<arg>);` as the first statement of the phase.
std::vector<core::FixItHint> superCallFix(const slang::SourceManager* sm, const slang::ast::SubroutineSymbol& sub) {
    using namespace slang::syntax;
    std::vector<core::FixItHint> fixes;
    const auto* syn = sub.getSyntax();
    if (!sm || !syn || (syn->kind != SyntaxKind::FunctionDeclaration && syn->kind != SyntaxKind::TaskDeclaration))
        return fixes;
    const auto& decl = syn->as<FunctionDeclarationSyntax>();
    const auto args = sub.getArguments();
    if (args.size() != 1) return fixes;
    // After the header and any leading declarations: SystemVerilog does not
    // allow a declaration after a statement.
    auto at = decl.semi.location() + decl.semi.rawText().size();
    std::string indent = lineIndent(*sm, decl.prototype->getFirstToken().location()) + "    ";
    for (const auto* item : decl.items) {
        if (StatementSyntax::isKind(item->kind)) break;
        const auto last = item->getLastToken();
        at = last.location() + last.rawText().size();
    }
    if (!decl.items.empty()) indent = lineIndent(*sm, decl.items[0]->getFirstToken().location());
    if (!at.valid() || sm->isMacroLoc(at)) return fixes;
    fixes.push_back({slang::SourceRange(at, at),
                     "\n" + indent + "super." + std::string(sub.name) + "(" + std::string(args[0]->name) + ");",
                     "Call super." + std::string(sub.name) + "() first",
                     core::FixSafety::Unsafe});
    return fixes;
}

} // namespace

class UvmMissingSuperPhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-001"; }
    std::string_view getName() const override { return "UvmMissingSuperPhase"; }
    std::string_view getDescription() const override {
        return "Overridden standard UVM phases (e.g., build_phase, connect_phase) in uvm_component "
               "must call super.<phase_name>(phase) to ensure proper base-class configuration.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            // Do not check the root uvm_component base class itself
            if (classType->name == "uvm_component") continue;
            if (!uvmModel.isUvmComponent(*classType)) continue;

            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                if (uvmModel.isStandardUvmPhase(sub.name)) {
                    const auto& body = sub.getBody();

                    SuperCallCheckVisitor visitor(sub.name);
                    body.visit(visitor);

                    if (!visitor.foundSuperCall()) {
                        diags.addViolation(
                            "UVM-PHS-001",
                            "UvmMissingSuperPhase",
                            "Method '" + std::string(sub.name) + "' in class '" +
                            std::string(classType->name) + "' does not call super." +
                            std::string(sub.name) + "(phase).",
                            core::Severity::Warning,
                            sub.location,
                            // Unsafe: the base phase then runs (in build_phase, automatic
                            // configuration from uvm_config_db), which changes behaviour.
                            superCallFix(compilation.getSourceManager(), sub)
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmMissingSuperPhase() {
    return std::make_unique<uvm::UvmMissingSuperPhase>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmMissingSuperPhase)
