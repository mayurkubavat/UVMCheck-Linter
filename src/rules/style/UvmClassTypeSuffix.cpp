#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <vector>
#include <initializer_list>

namespace uvmcheck::rules::style {

class UvmClassSuffixVisitor : public slang::syntax::SyntaxVisitor<UvmClassSuffixVisitor> {
public:
    explicit UvmClassSuffixVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        if (cls.extendsClause && cls.extendsClause->baseName) {
            std::string baseName = std::string(cls.extendsClause->baseName->getLastToken().valueText());
            std::string className = std::string(cls.name.valueText());

            // Ignore library / internal UVM classes
            if (className.starts_with("uvm_")) {
                visitDefault(cls);
                return;
            }

            if (baseName == "uvm_sequence_item" || baseName.find("uvm_sequence_item") != std::string::npos) {
                checkSuffix(cls, className, baseName,
                            {"_item", "_tr", "_trans", "_pkt", "Item", "Transaction", "Packet", "Tr"},
                            "'_item' or '_tr'");
            } else if (baseName == "uvm_sequence" || (baseName.find("uvm_sequence") != std::string::npos &&
                                                      baseName.find("sequence_item") == std::string::npos)) {
                checkSuffix(cls, className, baseName,
                            {"_seq", "_sequence", "_vseq", "Seq", "Sequence", "Vseq"},
                            "'_seq'");
            } else if (baseName == "uvm_driver" || baseName.find("uvm_driver") != std::string::npos) {
                checkSuffix(cls, className, baseName,
                            {"_driver", "_drv", "Driver", "Drv"},
                            "'_driver' or '_drv'");
            } else if (baseName == "uvm_monitor" || baseName.find("uvm_monitor") != std::string::npos) {
                checkSuffix(cls, className, baseName,
                            {"_monitor", "_mon", "Monitor", "Mon"},
                            "'_monitor' or '_mon'");
            } else if (baseName == "uvm_scoreboard" || baseName.find("uvm_scoreboard") != std::string::npos) {
                checkSuffix(cls, className, baseName,
                            {"_scoreboard", "_scb", "Scoreboard", "Scb"},
                            "'_scoreboard' or '_scb'");
            } else if (baseName == "uvm_agent" || baseName.find("uvm_agent") != std::string::npos) {
                checkSuffix(cls, className, baseName,
                            {"_agent", "_agt", "Agent", "Agt"},
                            "'_agent' or '_agt'");
            } else if (baseName == "uvm_env" || baseName.find("uvm_env") != std::string::npos) {
                checkSuffix(cls, className, baseName,
                            {"_env", "Env"},
                            "'_env'");
            } else if (baseName == "uvm_test" || baseName.find("uvm_test") != std::string::npos) {
                checkSuffix(cls, className, baseName,
                            {"_test", "Test"},
                            "'_test'");
            }
        }

        visitDefault(cls);
    }

private:
    void checkSuffix(const slang::syntax::ClassDeclarationSyntax& cls,
                     const std::string& className,
                     const std::string& baseName,
                     std::initializer_list<std::string_view> allowedSuffixes,
                     std::string_view recommendedSuffix) {
        bool matches = false;
        for (const auto& suffix : allowedSuffixes) {
            if (className.size() >= suffix.size() &&
                className.compare(className.size() - suffix.size(), suffix.size(), suffix) == 0) {
                matches = true;
                break;
            }
        }

        if (!matches) {
            diags.addViolation(
                "SV-NAM-005",
                "UvmClassTypeSuffix",
                "Class '" + className + "' extends '" + baseName +
                "' but does not end with recommended verification suffix " + std::string(recommendedSuffix) +
                ". Standardizing UVM class suffixes improves component discoverability and codebase readability.",
                core::Severity::Warning,
                cls.name.location()
            );
        }
    }

    core::DiagnosticEngine& diags;
};

class UvmClassTypeSuffix : public Rule {
public:
    std::string_view getId() const override { return "SV-NAM-005"; }
    std::string_view getName() const override { return "UvmClassTypeSuffix"; }
    std::string_view getDescription() const override {
        return "Enforces standard verification naming suffix conventions for classes extending UVM base components (e.g. _seq, _item, _driver, _monitor, _scoreboard, _env).";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UvmClassSuffixVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmClassTypeSuffix() {
    return std::make_unique<style::UvmClassTypeSuffix>();
}
} // namespace uvmcheck::rules
