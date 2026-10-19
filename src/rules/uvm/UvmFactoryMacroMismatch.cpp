#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/parsing/PreprocessorMetadata.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules::uvm {

class UvmFactoryMacroMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-FAC-002"; }
    std::string_view getName() const override { return "UvmFactoryMacroMismatch"; }
    std::string_view getDescription() const override {
        return "Classes derived from uvm_component must use `uvm_component_utils, and "
               "classes derived from uvm_object (non-component) must use `uvm_object_utils.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const auto& classes = uvmModel.getAllClasses();

        for (const auto* cls : classes) {
            if (!cls) continue;

            const auto* syntax = cls->getSyntax();
            if (!syntax) continue;

            auto classRange = syntax->sourceRange();
            bool isComp = uvmModel.isUvmComponent(*cls);
            bool isObj = uvmModel.isUvmObject(*cls);

            if (!isComp && !isObj) {
                // Not a UVM class
                continue;
            }

            // Check macro references occurring within this class definition
            for (const auto& tree : compilation.getSyntaxTrees()) {
                for (const auto& mref : tree->getMacroRefs()) {
                    if (!mref.syntax || !mref.definition) continue;

                    auto mLoc = mref.syntax->sourceRange().start();
                    // Check if macro invocation is inside this class
                    if (mLoc < classRange.start() || mLoc >= classRange.end()) {
                        continue;
                    }

                    std::string_view mName = mref.definition->name.valueText();

                    // Case 1: Component using uvm_object_utils*
                    if (isComp && mName.starts_with("uvm_object_utils")) {
                        diags.addViolation(
                            "UVM-FAC-002",
                            "UvmFactoryMacroMismatch",
                            "Class '" + std::string(cls->name) + "' derives from 'uvm_component' "
                            "but declares '`" + std::string(mName) + "'. Use '`uvm_component_utils' "
                            "for components to support hierarchical factory registration.",
                            core::Severity::Error,
                            mLoc
                        );
                    }
                    // Case 2: Object (non-component) using uvm_component_utils*
                    else if (!isComp && isObj && mName.starts_with("uvm_component_utils")) {
                        diags.addViolation(
                            "UVM-FAC-002",
                            "UvmFactoryMacroMismatch",
                            "Class '" + std::string(cls->name) + "' derives from 'uvm_object' (not a component) "
                            "but declares '`" + std::string(mName) + "'. Use '`uvm_object_utils' "
                            "because non-component objects do not have parent hierarchy contexts.",
                            core::Severity::Error,
                            mLoc
                        );
                    }
                }
            }

            // Also check type_id alias member if present (e.g. in expanded AST)
            for (const auto& member : cls->members()) {
                if (member.name == "type_id") {
                    std::string typeStr = member.as<slang::ast::Type>().toString();
                    if (isComp && typeStr.find("uvm_object_registry") != std::string::npos) {
                        diags.addViolation(
                            "UVM-FAC-002",
                            "UvmFactoryMacroMismatch",
                            "Class '" + std::string(cls->name) + "' derives from 'uvm_component' "
                            "but its factory registry is 'uvm_object_registry'. Use 'uvm_component_registry'.",
                            core::Severity::Error,
                            member.location
                        );
                    } else if (!isComp && isObj && typeStr.find("uvm_component_registry") != std::string::npos) {
                        diags.addViolation(
                            "UVM-FAC-002",
                            "UvmFactoryMacroMismatch",
                            "Class '" + std::string(cls->name) + "' derives from 'uvm_object' "
                            "but its factory registry is 'uvm_component_registry'. Use 'uvm_object_registry'.",
                            core::Severity::Error,
                            member.location
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmFactoryMacroMismatch() {
    return std::make_unique<uvm::UvmFactoryMacroMismatch>();
}
}
