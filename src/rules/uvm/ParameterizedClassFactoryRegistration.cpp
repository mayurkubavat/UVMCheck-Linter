#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/parsing/PreprocessorMetadata.h"
#include <string>

namespace uvmcheck::rules::uvm {

class ParamClassVisitor : public slang::syntax::SyntaxVisitor<ParamClassVisitor> {
public:
    explicit ParamClassVisitor(slang::ast::Compilation& compilation,
                               core::DiagnosticEngine& diags)
        : compilation(compilation), diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cdecl) {
        if (!cdecl.parameters) {
            visitDefault(cdecl);
            return;
        }

        bool isUvm = false;
        bool isComp = false;
        if (cdecl.extendsClause) {
            std::string baseText = std::string(cdecl.extendsClause->toString());
            if (baseText.find("uvm_") != std::string::npos) {
                isUvm = true;
                if (baseText.find("driver") != std::string::npos ||
                    baseText.find("agent") != std::string::npos ||
                    baseText.find("env") != std::string::npos ||
                    baseText.find("component") != std::string::npos ||
                    baseText.find("scoreboard") != std::string::npos) {
                    isComp = true;
                }
            }
        }

        if (isUvm) {
            auto classRange = cdecl.sourceRange();
            std::string clsName = std::string(cdecl.name.valueText());

            for (const auto& tree : compilation.getSyntaxTrees()) {
                if (!tree) continue;
                for (const auto& mref : tree->getMacroRefs()) {
                    if (!mref.syntax || !mref.definition) continue;

                    auto mLoc = mref.syntax->sourceRange().start();
                    if (mLoc < classRange.start() || mLoc >= classRange.end()) {
                        continue;
                    }

                    std::string_view mName = mref.definition->name.valueText();
                    bool isNonParamUtils = (mName == "uvm_component_utils" ||
                                            mName == "uvm_object_utils" ||
                                            mName == "uvm_component_utils_begin" ||
                                            mName == "uvm_object_utils_begin");

                    if (isNonParamUtils) {
                        std::string expectedMacro = isComp ? "`uvm_component_param_utils" : "`uvm_object_param_utils";
                        diags.addViolation(
                            "UVM-FAC-004",
                            "ParameterizedClassFactoryRegistration",
                            "Parameterized class '" + clsName + "' declares standard non-parameterized macro '`" +
                            std::string(mName) + "'. In UVM methodology (IEEE 1800.2 § 5.3), "
                            "parameterized classes must use '" + expectedMacro + "(...)' to ensure proper specialization and prevent "
                            "factory type registry name collisions.",
                            core::Severity::Error,
                            mLoc
                        );
                    }
                }
            }
        }
        visitDefault(cdecl);
    }

private:
    slang::ast::Compilation& compilation;
    core::DiagnosticEngine& diags;
};

class ParameterizedClassFactoryRegistration : public Rule {
public:
    std::string_view getId() const override { return "UVM-FAC-004"; }
    std::string_view getName() const override { return "ParameterizedClassFactoryRegistration"; }
    std::string_view getDescription() const override {
        return "Parameterized UVM components and objects must use `uvm_component_param_utils or `uvm_object_param_utils instead of non-parameterized utils macros.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ParamClassVisitor visitor(compilation, diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createParameterizedClassFactoryRegistration() {
    return std::make_unique<uvm::ParameterizedClassFactoryRegistration>();
}
} // namespace uvmcheck::rules
