#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::performance {

class PassArrayByValue : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-001"; }
    std::string_view getName() const override { return "PassArrayByValue"; }
    std::string_view getDescription() const override {
        return "Passing dynamic arrays, queues, associative arrays, unpacked arrays, or unpacked structs "
               "by value (input) causes expensive memory duplication on every call. Use 'const ref' or 'ref' instead.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const model::Class& cls : uvmModel.facade().classes()) {
            if (cls.isParameterized) continue;
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern) continue; // as before the port: in-class bodies only
                if (sub.isBuiltIn) continue;
                for (const model::Argument& arg : sub.arguments) {
                    if (arg.direction != model::Direction::In || arg.shape == model::TypeShape::Other) continue;
                    diags.addViolation(
                        "SV-PRF-001",
                        "PassArrayByValue",
                        "Argument '" + arg.name + "' of type '" + arg.canonicalType + "' in method '" +
                        sub.name + "' is passed by value (input). Use 'const ref' instead.",
                        core::Severity::Warning,
                        model::toSlang(arg.loc)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPassArrayByValue() {
    return std::make_unique<performance::PassArrayByValue>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createPassArrayByValue)
