#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/syntax/SyntaxNode.h"
#include <unordered_map>
#include <algorithm>

namespace uvmcheck::rules::oop {

class DuplicateMethodBody : public Rule {
public:
    std::string_view getId() const override { return "SV-DUP-001"; }
    std::string_view getName() const override { return "DuplicateMethodBody"; }
    std::string_view getDescription() const override {
        return "Detects identical or copy-pasted subroutine method bodies across different classes. "
               "Duplicated code should be refactored into shared base classes or utilities.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        struct MethodOrigin {
            std::string className;
            std::string methodName;
            slang::SourceLocation location;
        };

        // Key: normalized method body text; Value: first discovered occurrence
        std::unordered_map<std::string, MethodOrigin> knownBodies;

        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            for (const auto& member : cls->members()) {
                if (member.kind != slang::ast::SymbolKind::Subroutine) continue;

                const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;
                if (sub.name == "new") continue; // constructors with super.new() are standard

                const auto* syntax = sub.getSyntax();
                if (!syntax) continue;

                auto range = syntax->sourceRange();
                if (!sm->isFileLoc(range.start()) || !sm->isFileLoc(range.end())) continue;

                auto fileId = range.start().buffer();
                std::string_view fileText = sm->getSourceText(fileId);
                size_t startOff = range.start().offset();
                size_t endOff = range.end().offset();

                if (endOff <= startOff || endOff > fileText.size()) continue;

                std::string_view rawBody = fileText.substr(startOff, endOff - startOff);

                // Normalize: strip whitespace and newlines to compare core statement tokens
                std::string normalized;
                normalized.reserve(rawBody.size());
                bool inLineComment = false;
                for (size_t i = 0; i < rawBody.size(); ++i) {
                    char c = rawBody[i];
                    if (inLineComment) {
                        if (c == '\n') inLineComment = false;
                        continue;
                    }
                    if (c == '/' && i + 1 < rawBody.size() && rawBody[i + 1] == '/') {
                        inLineComment = true;
                        continue;
                    }
                    if (c != ' ' && c != '\t' && c != '\r' && c != '\n') {
                        normalized += c;
                    }
                }

                // Skip small/trivial bodies (e.g. empty or 1-liners under 50 non-whitespace chars)
                if (normalized.size() < 50) continue;

                auto it = knownBodies.find(normalized);
                if (it != knownBodies.end()) {
                    // Avoid reporting duplicate against itself in the same class
                    if (it->second.className != cls->name) {
                        diags.addViolation(
                            "SV-DUP-001",
                            "DuplicateMethodBody",
                            "Subroutine '" + std::string(sub.name) + "' in class '" + std::string(cls->name) +
                            "' has an identical body to subroutine '" + it->second.methodName +
                            "' in class '" + it->second.className +
                            "'. Refactor duplicate code into a common base class or shared package utility.",
                            core::Severity::Warning,
                            sub.location
                        );
                    }
                } else {
                    knownBodies[normalized] = MethodOrigin{
                        std::string(cls->name),
                        std::string(sub.name),
                        sub.location
                    };
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDuplicateMethodBody() {
    return std::make_unique<oop::DuplicateMethodBody>();
}
}
