#include "uvmcheck/compat/Overlay.h"

#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/rules/CompilerDiagnostics.h"

#include <map>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include "slang/ast/Compilation.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::compat {

namespace {

using LineRange = std::pair<size_t, size_t>;

/// Line spans of every task and function, by file.
class SubroutineSpans : public slang::syntax::SyntaxVisitor<SubroutineSpans> {
public:
    explicit SubroutineSpans(const slang::SourceManager& sm) : sm(sm) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& fn) {
        auto range = fn.sourceRange();
        auto start = sm.getFullyOriginalLoc(range.start()), end = sm.getFullyOriginalLoc(range.end());
        if (start.valid() && end.valid() && start.buffer() == end.buffer())
            spans[std::string(sm.getFileName(start))].emplace_back(sm.getLineNumber(start), sm.getLineNumber(end));
        visitDefault(fn);
    }

    const slang::SourceManager& sm;
    std::map<std::string, std::vector<LineRange>> spans;
};

/// When a task or function does not compile, slang keeps what it can of its
/// body and drops the rest, and which parts survive changes between slang
/// releases. Rules reading such a body then report things that are not there
/// (an argument "never used" because its type did not resolve, a super call
/// "missing" because the body around an illegal delay was dropped). This
/// downgrades findings inside such a subroutine to Info, so the compiler error
/// is the one to fix first. Findings on the error's own line are kept, since
/// those rules are pointing at the illegal construct itself.
class ErrorTaint : public Override {
public:
    std::string_view id() const override { return "error-taint"; }
    std::string_view summary() const override {
        return "Downgrade findings to Info inside a task or function that has a compile error, where they may be "
               "artifacts of slang's error recovery";
    }

    void reviewFindings(slang::ast::Compilation& compilation, std::vector<core::Violation>& findings) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        std::map<std::string, std::set<size_t>> errorLines;
        for (const auto& diag : compilation.getAllDiagnostics()) {
            if (!diag.isError() || !diag.location.valid()) continue;
            auto loc = sm->getFullyOriginalLoc(diag.location);
            errorLines[std::string(sm->getFileName(loc))].insert(sm->getLineNumber(loc));
        }
        if (errorLines.empty()) return;

        SubroutineSpans spans(*sm);
        for (const auto& tree : compilation.getSyntaxTrees()) tree->root().visit(spans);

        for (auto& v : findings) {
            if (rules::isCompilerDiagnosticRule(v.ruleId) || v.severity == core::Severity::Info) continue;
            auto errs = errorLines.find(v.filePath);
            auto fileSpans = spans.spans.find(v.filePath);
            if (errs == errorLines.end() || fileSpans == spans.spans.end()) continue;
            if (errs->second.count(v.line)) continue;
            for (const auto& [first, last] : fileSpans->second) {
                if (v.line < first || v.line > last) continue;
                auto e = errs->second.lower_bound(first);
                if (e == errs->second.end() || *e > last) continue;
                v.severity = core::Severity::Info;
                v.message += " [error-taint: line " + std::to_string(*e) +
                             " of this subroutine does not compile; fix that first]";
                break;
            }
        }
    }
};

} // namespace

std::unique_ptr<Override> createErrorTaint() {
    return std::make_unique<ErrorTaint>();
}

} // namespace uvmcheck::compat
