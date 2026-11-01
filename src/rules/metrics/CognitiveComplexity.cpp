#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

class CognitiveComplexity : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-001"; }
    std::string_view getName() const override { return "CognitiveComplexity"; }
    std::string_view getDescription() const override {
        return "Flags routines whose Sonar cognitive complexity (nesting-weighted branching, with fork/join "
               "counted as nesting) exceeds the threshold (default 25; Sonar S3776).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-001", "threshold", 25);
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            auto m = computeRoutineMetrics(*sub);
            if (m.cognitive <= limit) continue;
            diags.addViolation(
                "SV-MET-001", "CognitiveComplexity",
                "'" + std::string(sub->name) + "' has cognitive complexity " + std::to_string(m.cognitive) +
                    " (limit " + std::to_string(limit) + "). Deeply nested forks, loops and branches are "
                    "hard to review and to cover; extract the nested parts into named tasks.",
                core::Severity::Warning, sub->location);
        }
    }
};

class LongTask : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-007"; }
    std::string_view getName() const override { return "LongTask"; }
    std::string_view getDescription() const override {
        return "Flags tasks longer than the line budget (default 40 source lines, option max_lines). "
               "A long task is hard to review and to cover; split it into named tasks with one job each.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-007", "max_lines", 40);
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            if (sub->subroutineKind != SubroutineKind::Task) continue;
            int lines = routineLineSpan(*sub);
            if (lines <= limit) continue;
            std::string where(sub->name);
            if (const Scope* scope = sub->getParentScope()) {
                const Symbol& owner = scope->asSymbol();
                if (!owner.name.empty() && owner.name != "$unit")
                    where = std::string(owner.name) + "::" + std::string(sub->name);
            }
            diags.addViolation(
                "SV-MET-007", "LongTask",
                "Task '" + where + "' is " + std::to_string(lines) + " lines (limit " +
                    std::to_string(limit) + "). Split it into smaller tasks.",
                core::Severity::Warning, sub->location);
        }
    }
};

class RandFieldBudget : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-008"; }
    std::string_view getName() const override { return "RandFieldBudget"; }
    std::string_view getDescription() const override {
        return "Flags a sequence item that declares more than the rand-field budget (default 8, option "
               "max_rand), counting rand and randc properties of the class itself. A wide item is slow "
               "to solve and hard to constrain; split the fields across items.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-008", "max_rand", 8);
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Item) continue;
            const auto* syntax = cls->getSyntax();
            if (!syntax || syntax->kind != slang::syntax::SyntaxKind::ClassDeclaration) continue;
            int n = 0;
            slang::SourceLocation loc = cls->location;
            for (const auto* member : syntax->as<slang::syntax::ClassDeclarationSyntax>().items) {
                if (member->kind != slang::syntax::SyntaxKind::ClassPropertyDeclaration) continue;
                const auto& prop = member->as<slang::syntax::ClassPropertyDeclarationSyntax>();
                bool random = false;
                for (const auto& q : prop.qualifiers) {
                    if (q.kind == slang::parsing::TokenKind::RandKeyword ||
                        q.kind == slang::parsing::TokenKind::RandCKeyword)
                        random = true;
                }
                if (!random || !prop.declaration ||
                    prop.declaration->kind != slang::syntax::SyntaxKind::DataDeclaration)
                    continue;
                for (const auto* decl : prop.declaration->as<slang::syntax::DataDeclarationSyntax>().declarators) {
                    n++;
                    if (n == static_cast<int>(limit) + 1) loc = decl->name.location();
                }
            }
            if (n <= limit) continue;
            diags.addViolation(
                "SV-MET-008", "RandFieldBudget",
                "Sequence item '" + std::string(cls->name) + "' declares " + std::to_string(n) +
                    " rand fields (limit " + std::to_string(limit) +
                    "). Split the item, or mark fields that the solver should not choose as non-rand.",
                core::Severity::Warning, loc);
        }
    }
};

} // namespace uvmcheck::rules::metrics

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCognitiveComplexity() {
    return std::make_unique<metrics::CognitiveComplexity>();
}
std::unique_ptr<Rule> createLongTask() { return std::make_unique<metrics::LongTask>(); }
std::unique_ptr<Rule> createRandFieldBudget() { return std::make_unique<metrics::RandFieldBudget>(); }
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createCognitiveComplexity)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createLongTask)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRandFieldBudget)
