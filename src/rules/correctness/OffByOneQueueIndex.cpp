#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::correctness {

class OffByOneQueueVisitor : public slang::syntax::SyntaxVisitor<OffByOneQueueVisitor> {
public:
    explicit OffByOneQueueVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ElementSelectSyntax& elem) {
        std::string elemStr = trim(std::string(elem.toString()));

        // Check if selector bracket contains .size without subtraction
        if (elemStr.find(".size") != std::string::npos) {
            if (elemStr.find('-') == std::string::npos &&
                elemStr.find('+') == std::string::npos) {
                diags.addViolation(
                    "SV-ARR-001",
                    "OffByOneQueueIndex",
                    "Out-of-bounds array access '" + elemStr + "'. "
                    "In SystemVerilog (IEEE 1800 § 7.10.1), queue and dynamic array indices span from 0 to size()-1. "
                    "Accessing index size() is an off-by-one error that silently returns an uninitialized value ('x or 0) "
                    "without triggering a simulator runtime error. Did you mean index [size() - 1]?",
                    core::Severity::Error,
                    elem.sourceRange().start()
                );
            }
        }

        visitDefault(elem);
    }

private:
    static std::string trim(const std::string& str) {
        size_t first = str.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) return "";
        size_t last = str.find_last_not_of(" \t\r\n");
        return str.substr(first, last - first + 1);
    }

    core::DiagnosticEngine& diags;
};

class OffByOneQueueIndex : public Rule {
public:
    std::string_view getId() const override { return "SV-ARR-001"; }
    std::string_view getName() const override { return "OffByOneQueueIndex"; }
    std::string_view getDescription() const override {
        return "Detects out-of-bounds off-by-one queue and dynamic array indexing (e.g. 'q[q.size()]'). "
               "SystemVerilog silently returns uninitialized 'x/0 instead of raising an exception.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        OffByOneQueueVisitor visitor(diags);
        for (const auto& syntaxTree : compilation.getSyntaxTrees()) {
            if (syntaxTree) {
                syntaxTree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createOffByOneQueueIndex() {
    return std::make_unique<correctness::OffByOneQueueIndex>();
}
}
