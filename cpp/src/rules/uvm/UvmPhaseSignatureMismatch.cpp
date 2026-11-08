#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class UvmPhaseSignatureMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-003"; }
    std::string_view getName() const override { return "UvmPhaseSignatureMismatch"; }
    std::string_view getDescription() const override {
        return "Standard UVM phase methods must declare correct signatures (function void vs task, "
               "and a single argument of type uvm_phase).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized || !model::isComponent(cls.uvm)) continue;

            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern) continue; // as before the port: in-class bodies only
                if (sub.isBuiltIn) continue;

                const std::string& subName = sub.name;
                bool isFuncPhase = uvmcheck::uvm::UvmModel::isFunctionPhase(subName);
                bool isTskPhase = uvmcheck::uvm::UvmModel::isTaskPhase(subName);

                if (!isFuncPhase && !isTskPhase) continue;
                const auto where = model::toSlang(sub.loc);

                // Check Function Phase rules
                if (isFuncPhase) {
                    if (sub.isTask) {
                        diags.addViolation(
                            "UVM-PHS-003",
                            "UvmPhaseSignatureMismatch",
                            "UVM phase '" + subName + "' in class '" + cls.name +
                            "' is declared as a 'task'. Standard UVM function phases must be declared as 'function void'.",
                            core::Severity::Error,
                            where
                        );
                    } else if (!sub.returnsVoid) {
                        diags.addViolation(
                            "UVM-PHS-003",
                            "UvmPhaseSignatureMismatch",
                            "UVM phase '" + subName + "' in class '" + cls.name +
                            "' has return type '" + sub.returnType +
                            "'. Standard UVM phases must return 'void'.",
                            core::Severity::Error,
                            where
                        );
                    }
                }

                // Check Task Phase rules
                if (isTskPhase && !sub.isTask) {
                    diags.addViolation(
                        "UVM-PHS-003",
                        "UvmPhaseSignatureMismatch",
                        "UVM phase '" + subName + "' in class '" + cls.name +
                        "' is declared as a 'function'. Time-consuming UVM phases must be declared as 'task'.",
                        core::Severity::Error,
                        where
                    );
                }

                // Check argument count and type
                if (sub.arguments.size() != 1) {
                    diags.addViolation(
                        "UVM-PHS-003",
                        "UvmPhaseSignatureMismatch",
                        "UVM phase '" + subName + "' in class '" + cls.name +
                        "' declares " + std::to_string(sub.arguments.size()) +
                        " argument(s), but standard UVM phases must declare exactly one argument of type 'uvm_phase'.",
                        core::Severity::Error,
                        where
                    );
                } else {
                    const model::Argument& arg = sub.arguments.front();
                    bool isPhaseType = (arg.typeClass != model::NoClass &&
                                        design.derivesFrom(arg.typeClass, "uvm_phase")) ||
                                       arg.type.find("uvm_phase") != std::string::npos;
                    if (!isPhaseType) {
                        diags.addViolation(
                            "UVM-PHS-003",
                            "UvmPhaseSignatureMismatch",
                            "Argument '" + arg.name + "' of UVM phase '" + subName +
                            "' in class '" + cls.name + "' has type '" +
                            arg.type + "', but must be of type 'uvm_phase'.",
                            core::Severity::Error,
                            model::toSlang(arg.loc)
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmPhaseSignatureMismatch() {
    return std::make_unique<uvm::UvmPhaseSignatureMismatch>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmPhaseSignatureMismatch)
