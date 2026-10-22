#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/text/SourceManager.h"

#include <set>

namespace uvmcheck::rules::readability {

using namespace slang::ast;

namespace {

bool isDisplayCall(std::string_view n) {
    return n.starts_with("$display") || n.starts_with("$write") || n.starts_with("$sformat") ||
           n.starts_with("$fdisplay") || n.starts_with("$fwrite") || n == "$psprintf" || n == "$swrite";
}

class MagicVisitor : public ASTVisitor<MagicVisitor, VisitFlags::AllGood> {
public:
    MagicVisitor(const slang::SourceManager* sm, const std::string& where, core::DiagnosticEngine& diags) :
        sm(sm), where(where), diags(diags) {}

    void handle(const IntegerLiteral& lit) {
        auto v = lit.getValue().as<int64_t>();
        if (!v || *v == 0 || *v == 1 || *v == -1) return;
        slang::SourceLocation loc = lit.sourceRange.start();
        if (sm && sm->isMacroLoc(loc)) return;
        if (!reported.insert(*v).second) return;
        diags.addViolation(
            "SV-RDB-002", "MagicNumber",
            "Magic number " + std::to_string(*v) + " in " + where +
                ". Give it a name (localparam, enum or config field) so its meaning and every place "
                "that depends on it are visible.",
            core::Severity::Info, loc);
    }

    void handle(const CallExpression& call) {
        // Report text and verbosity arguments are not protocol constants.
        if (swe::reportLevel(call) != swe::ReportLevel::None) return;
        if (call.isSystemCall() && isDisplayCall(call.getSubroutineName())) return;
        visitDefault(call);
    }

    // Delays are left to the magic-delay rules (SV-SML-002, UVM-TST-002).
    void handle(const DelayControl&) {}

    void handle(const VariableSymbol& var) {
        // const locals are the named constants this rule asks for.
        if (var.flags.has(VariableFlags::Const)) return;
        visitDefault(var);
    }

private:
    const slang::SourceManager* sm;
    const std::string& where;
    core::DiagnosticEngine& diags;
    std::set<int64_t> reported;
};

} // namespace

class MagicNumber : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-002"; }
    std::string_view getName() const override { return "MagicNumber"; }
    std::string_view getDescription() const override {
        return "Flags numeric literals other than 0, 1 and -1 in class method bodies: conditions, loop "
               "bounds and sizes; delays are left to the magic-delay rules (Checkstyle MagicNumber, Sonar S109). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        for (const auto* cls : swe::userClasses(model)) {
            swe::Role role = swe::classRole(*cls);
            if (role == swe::Role::RegModel) continue;
            for (const auto* m : swe::classMethods(*cls)) {
                std::string where = std::string(cls->name) + "::" + std::string(m->name);
                MagicVisitor v(sm, where, diags);
                m->getBody().visit(v);
            }
        }
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMagicNumber() {
    return std::make_unique<readability::MagicNumber>();
}
}
