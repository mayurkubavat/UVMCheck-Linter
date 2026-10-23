#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/parsing/PreprocessorMetadata.h"

namespace uvmcheck::rules::uvm {

class UvmFactoryMissingUtils : public Rule {
public:
    std::string_view getId() const override { return "UVM-FAC-003"; }
    std::string_view getName() const override { return "UvmFactoryMissingUtils"; }
    std::string_view getDescription() const override {
        return "Concrete classes derived from uvm_object or uvm_component must declare "
               "factory registration macros (`uvm_component_utils / `uvm_object_utils).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const auto& classes = uvmModel.getAllClasses();

        for (const auto* cls : classes) {
            if (!cls) continue;
            // Abstract classes and interface classes do not require factory utils
            if (cls->isAbstract || cls->isInterface) continue;

            // Skip internal/library UVM base classes
            if (cls->name.starts_with("uvm_")) continue;

            bool isComp = uvmModel.isUvmComponent(*cls);
            bool isObj = uvmModel.isUvmObject(*cls);

            if (!isComp && !isObj) continue;

            const auto* syntax = cls->getSyntax();
            if (!syntax) continue;

            auto classRange = syntax->sourceRange();
            bool hasRegistration = false;

            // Check if the class itself declares type_id or get_type. find() also returns
            // inherited members, and the real uvm_object/uvm_component declare get_type().
            if (declaresOwn(*cls, "type_id") || declaresOwn(*cls, "get_type")) {
                hasRegistration = true;
            }

            // Check if any factory macro was invoked in the class body
            if (!hasRegistration) {
                for (const auto& tree : compilation.getSyntaxTrees()) {
                    for (const auto& mref : tree->getMacroRefs()) {
                        if (!mref.syntax || !mref.definition) continue;

                        auto mLoc = mref.syntax->sourceRange().start();
                        if (mLoc >= classRange.start() && mLoc < classRange.end()) {
                            std::string_view mName = mref.definition->name.valueText();
                            if (mName.starts_with("uvm_component_utils") ||
                                mName.starts_with("uvm_object_utils") ||
                                mName.starts_with("uvm_component_param_utils") ||
                                mName.starts_with("uvm_object_param_utils")) {
                                hasRegistration = true;
                                break;
                            }
                        }
                    }
                    if (hasRegistration) break;
                }
            }

            if (!hasRegistration) {
                std::string baseKind = isComp ? "uvm_component" : "uvm_object";
                std::string recommendedMacro = isComp ? "`uvm_component_utils" : "`uvm_object_utils";

                diags.addViolation(
                    "UVM-FAC-003",
                    "UvmFactoryMissingUtils",
                    "Concrete class '" + std::string(cls->name) + "' derived from '" + baseKind +
                    "' is not registered with the UVM factory. Add '" + recommendedMacro +
                    "(" + std::string(cls->name) + ")' to enable testbench factory overrides.",
                    core::Severity::Warning,
                    cls->location
                );
            }
        }
    }

private:
    static bool declaresOwn(const slang::ast::ClassType& cls, std::string_view name) {
        const auto* sym = cls.find(name);
        return sym && sym->getParentScope() == &cls;
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmFactoryMissingUtils() {
    return std::make_unique<uvm::UvmFactoryMissingUtils>();
}
}
