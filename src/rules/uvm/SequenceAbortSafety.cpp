#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::uvm {

class SequenceAbortVisitor : public engine::SyntaxVisitor<SequenceAbortVisitor> {
public:
    explicit SequenceAbortVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::string clsName = std::string(cls.name.valueText());
        std::string extendsText;
        if (cls.extendsClause) extendsText = std::string(cls.extendsClause->toString());

        bool isSequence = (extendsText.find("uvm_sequence") != std::string::npos) ||
                          (clsName.find("sequence") != std::string::npos) ||
                          (clsName.find("seq") != std::string::npos);

        if (isSequence) {
            for (const auto* member : cls.items) {
                const slang::syntax::FunctionDeclarationSyntax* routine = nullptr;
                if (member->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
                    routine = member->as<slang::syntax::ClassMethodDeclarationSyntax>().declaration;
                } else if (member->kind == slang::syntax::SyntaxKind::FunctionDeclaration) {
                    routine = &member->as<slang::syntax::FunctionDeclarationSyntax>();
                }

                if (routine && routine->prototype) {
                    std::string routineText = stripComments(std::string(routine->toString()));
                    size_t startPos = 0;
                    while ((startPos = routineText.find("start_item", startPos)) != std::string::npos) {
                        size_t finishPos = routineText.find("finish_item", startPos);
                        size_t returnPos = routineText.find("return", startPos);

                        // If return is followed by semicolon (early return statement)
                        if (returnPos != std::string::npos) {
                            size_t afterReturn = routineText.find_first_not_of(" \t", returnPos + 6);
                            if (afterReturn != std::string::npos && routineText[afterReturn] == ';') {
                                if (finishPos == std::string::npos || returnPos < finishPos) {
                                    std::string taskName = std::string(routine->prototype->name->getFirstToken().valueText());
                                    diags.addViolation(
                                        "UVM-SEQ-004",
                                        "SequenceAbortSafety",
                                        "Task '" + taskName + "' in sequence class '" + clsName +
                                        "' calls start_item() but contains an early 'return;' before finish_item(). "
                                        "Aborting a sequence handshake without calling finish_item() permanently leaves the driver-sequencer pipeline in an un-granted deadlock.",
                                        core::Severity::Error,
                                        routine->prototype->name->sourceRange().start()
                                    );
                                    break;
                                }
                            }
                        }
                        startPos += 10;
                    }
                }
            }
        }

        visitDefault(cls);
    }

private:
    static std::string stripComments(const std::string& input) {
        std::string result;
        result.reserve(input.size());
        bool inLineComment = false;
        for (size_t i = 0; i < input.size(); ++i) {
            if (!inLineComment && i + 1 < input.size() && input[i] == '/' && input[i+1] == '/') {
                inLineComment = true;
                ++i;
            } else if (inLineComment && input[i] == '\n') {
                inLineComment = false;
                result += '\n';
            } else if (!inLineComment) {
                result += input[i];
            }
        }
        return result;
    }

    core::DiagnosticEngine& diags;
};

class SequenceAbortSafety : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-004"; }
    std::string_view getName() const override { return "SequenceAbortSafety"; }
    std::string_view getDescription() const override {
        return "Detects calls to start_item() in UVM sequences that can bypass finish_item() via early return or break statements. "
               "An aborted sequence without finish_item() permanently deadlocks the sequencer's grant-wait queue.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }
    uint32_t getProfiles() const override {
        return static_cast<uint32_t>(RuleProfile::Soundness) | static_cast<uint32_t>(RuleProfile::Methodology);
    }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        SequenceAbortVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequenceAbortSafety() {
    return std::make_unique<uvm::SequenceAbortSafety>();
}
}
