#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>

namespace uvmcheck::rules::coverage {

class CrossSameCoverpointVisitor : public slang::syntax::SyntaxVisitor<CrossSameCoverpointVisitor> {
public:
    explicit CrossSameCoverpointVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverCrossSyntax& node) {
        std::unordered_set<std::string> seen;
        for (const auto* item : node.items) {
            if (!item) continue;
            std::string name = std::string(item->toString());
            // Trim whitespace
            name.erase(0, name.find_first_not_of(" \t\n\r"));
            if (size_t end = name.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                name.erase(end + 1);
            }
            if (!name.empty()) {
                if (seen.count(name)) {
                    std::string crossName = node.label ? std::string(node.label->name.valueText()) : "unnamed";
                    diags.addViolation(
                        "SV-COV-003",
                        "CrossSameCoverpoint",
                        "Coverage cross '" + crossName + "' crosses coverpoint '" + name +
                        "' with itself. Crossing identical coverpoints creates redundant diagonal bins and skews regression coverage metrics.",
                        core::Severity::Error,
                        item->sourceRange().start(),
                        {}
                    );
                } else {
                    seen.insert(name);
                }
            }
        }
        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class CrossSameCoverpoint : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-003"; }
    std::string_view getName() const override { return "CrossSameCoverpoint"; }
    std::string_view getDescription() const override {
        return "Detects coverage cross constructs crossing a coverpoint with itself (cross cp_a, cp_a). "
               "Crossing a coverpoint with itself creates redundant diagonal bins that distort coverage calculation.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        CrossSameCoverpointVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCrossSameCoverpoint() {
    return std::make_unique<coverage::CrossSameCoverpoint>();
}
}
