#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::random {

class ZeroWeightDistVisitor : public slang::syntax::SyntaxVisitor<ZeroWeightDistVisitor> {
public:
    explicit ZeroWeightDistVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DistConstraintListSyntax& node) {
        bool hasItems = false;
        bool allZero = true;

        for (const auto* item : node.items) {
            if (!item) continue;
            hasItems = true;
            if (const auto* distItem = item->as_if<slang::syntax::DistItemSyntax>()) {
                if (distItem->weight && distItem->weight->expr) {
                    std::string wStr = std::string(distItem->weight->expr->toString());
                    wStr.erase(0, wStr.find_first_not_of(" \t\n\r"));
                    if (size_t end = wStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        wStr.erase(end + 1);
                    }
                    if (wStr != "0" && wStr != "'0" && wStr != "32'd0" && wStr != "32'b0") {
                        allZero = false;
                        break;
                    }
                } else {
                    allZero = false;
                    break;
                }
            } else if (const auto* defItem = item->as_if<slang::syntax::DefaultDistItemSyntax>()) {
                if (defItem->weight && defItem->weight->expr) {
                    std::string wStr = std::string(defItem->weight->expr->toString());
                    wStr.erase(0, wStr.find_first_not_of(" \t\n\r"));
                    if (size_t end = wStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        wStr.erase(end + 1);
                    }
                    if (wStr != "0" && wStr != "'0" && wStr != "32'd0" && wStr != "32'b0") {
                        allZero = false;
                        break;
                    }
                } else {
                    allZero = false;
                    break;
                }
            } else {
                allZero = false;
                break;
            }
        }

        if (hasItems && allZero) {
            diags.addViolation(
                "SV-RND-012",
                "ZeroWeightDistribution",
                "All weights in constraint distribution ('dist { ... }') evaluate to zero. "
                "Per IEEE 1800 § 18.5.4, at least one weight in a distribution constraint must evaluate to a non-zero value; a distribution sum of zero creates an unsatisfiable constraint that triggers solver runtime failure.",
                core::Severity::Error,
                node.sourceRange().start()
            );
        }

        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class ZeroWeightDistribution : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-012"; }
    std::string_view getName() const override { return "ZeroWeightDistribution"; }
    std::string_view getDescription() const override {
        return "Detects constraint distribution blocks where all weights evaluate to zero. At least one "
               "weight in a distribution must evaluate to non-zero per IEEE 1800 § 18.5.4.";
    }
    std::string_view getCategory() const override { return "Random"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ZeroWeightDistVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createZeroWeightDistribution() {
    return std::make_unique<random::ZeroWeightDistribution>();
}
}
