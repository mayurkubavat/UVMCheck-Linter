#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/parsing/PreprocessorMetadata.h"
#include "slang/text/SourceManager.h"

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

                // Insert the macro after the class header. Parameterized classes need
                // the *_param_utils form with their parameters, so they get no fix.
                std::vector<core::FixItHint> fixes;
                const auto* sm = compilation.getSourceManager();
                if (sm && syntax->kind == slang::syntax::SyntaxKind::ClassDeclaration) {
                    const auto& decl = syntax->as<slang::syntax::ClassDeclarationSyntax>();
                    const auto semi = decl.semi.location();
                    if (!decl.parameters && semi.valid() && !sm->isMacroLoc(semi)) {
                        std::string indent = "    ";
                        if (!decl.items.empty()) {
                            std::string_view text = sm->getSourceText(semi.buffer());
                            size_t pos = decl.items[0]->getFirstToken().location().offset();
                            size_t start = pos;
                            while (start > 0 && text[start - 1] != '\n') --start;
                            size_t end = start;
                            while (end < pos && (text[end] == ' ' || text[end] == '\t')) ++end;
                            indent = std::string(text.substr(start, end - start));
                        }
                        const auto at = semi + 1;
                        // Unsafe: get_type_name() and create() change, and `uvm_object_utils
                        // needs a constructor whose name argument has a default.
                        fixes.push_back({slang::SourceRange(at, at),
                                         "\n" + indent + recommendedMacro + "(" + std::string(cls->name) + ")",
                                         "Register with the factory", core::FixSafety::Unsafe});
                    }
                }
                diags.addViolation(
                    "UVM-FAC-003",
                    "UvmFactoryMissingUtils",
                    "Concrete class '" + std::string(cls->name) + "' derived from '" + baseKind +
                    "' is not registered with the UVM factory. Add '" + recommendedMacro +
                    "(" + std::string(cls->name) + ")' to enable testbench factory overrides.",
                    core::Severity::Warning,
                    cls->location,
                    fixes
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

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmFactoryMissingUtils)
