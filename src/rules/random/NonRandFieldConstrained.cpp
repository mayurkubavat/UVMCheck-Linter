#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::random {

class NonRandFieldConstrained : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-005"; }
    std::string_view getName() const override { return "NonRandFieldConstrained"; }
    std::string_view getDescription() const override {
        return "Detects class member fields that are constrained in a constraint block but not declared 'rand' or 'randc'. "
               "The solver treats non-rand fields as fixed constants and will never randomize them.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            // Map non-rand properties: name -> ClassPropertySymbol
            std::unordered_map<std::string, const slang::ast::ClassPropertySymbol*> nonRandProps;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    if (prop.getRandMode() == slang::ast::RandMode::None &&
                        prop.lifetime != slang::ast::VariableLifetime::Static) {
                        nonRandProps[std::string(prop.name)] = &prop;
                    }
                }
            }

            if (nonRandProps.empty()) continue;

            // Inspect constraint blocks
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ConstraintBlock) {
                    const auto& cblock = member.as<slang::ast::ConstraintBlockSymbol>();
                    const auto* cbSyntax = cblock.getSyntax();
                    if (!cbSyntax) continue;

                    std::string cblockText = std::string(cbSyntax->toString());

                    for (const auto& [propName, propSym] : nonRandProps) {
                        // Check if propName appears on LHS of comparison or inside operator
                        // e.g. "length inside", "length ==", "length <", "length >", "length <="
                        bool appearsConstrained = false;

                        std::string targetInside = propName + " inside";
                        std::string targetEq = propName + " ==";
                        std::string targetLt = propName + " <";
                        std::string targetGt = propName + " >";
                        std::string targetLe = propName + " <=";
                        std::string targetGe = propName + " >=";

                        if (cblockText.find(targetInside) != std::string::npos ||
                            cblockText.find(targetEq) != std::string::npos ||
                            cblockText.find(targetLt) != std::string::npos ||
                            cblockText.find(targetGt) != std::string::npos ||
                            cblockText.find(targetLe) != std::string::npos ||
                            cblockText.find(targetGe) != std::string::npos) {
                            appearsConstrained = true;
                        }

                        if (appearsConstrained) {
                            diags.addViolation(
                                "SV-RND-005",
                                "NonRandFieldConstrained",
                                "Class property '" + propName + "' in class '" + std::string(cls->name) +
                                "' is constrained in constraint block '" + std::string(cblock.name) +
                                "' but is not declared with the 'rand' or 'randc' keyword. In SystemVerilog "
                                "(IEEE 1800 § 18.4), non-rand fields are treated as fixed constants by the solver "
                                "and will never be randomized. Add 'rand' to the declaration of '" + propName + "'.",
                                core::Severity::Error,
                                propSym->location
                            );
                        }
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonRandFieldConstrained() {
    return std::make_unique<random::NonRandFieldConstrained>();
}
}
