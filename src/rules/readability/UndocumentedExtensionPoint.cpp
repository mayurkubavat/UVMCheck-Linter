#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/parsing/Token.h"
#include "slang/syntax/SyntaxNode.h"

#include <unordered_set>

namespace uvmcheck::rules::readability {

using namespace slang::ast;

namespace {

bool hasLeadingComment(const Symbol& member) {
    const auto* syntax = member.getSyntax();
    if (!syntax) return true; // nothing to check against
    // Method syntax hangs off the declaration; look at the first token of the whole member.
    const auto* node = syntax;
    if (node->parent && (node->parent->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration ||
                         node->parent->kind == slang::syntax::SyntaxKind::ClassMethodPrototype))
        node = node->parent;
    for (const auto& t : node->getFirstToken().trivia()) {
        if (t.kind == slang::parsing::TriviaKind::LineComment || t.kind == slang::parsing::TriviaKind::BlockComment)
            return true;
    }
    return false;
}

bool isUvmStandardName(std::string_view n) {
    return n == "new" || n.ends_with("_phase") || n.starts_with("do_") || n == "convert2string" ||
           n == "get_type_name" || n == "create" || n == "body" || n == "pre_body" || n == "post_body" ||
           n == "pre_start" || n == "post_start" || n == "write" || n == "catch";
}

} // namespace

class UndocumentedExtensionPoint : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-004"; }
    std::string_view getName() const override { return "UndocumentedExtensionPoint"; }
    std::string_view getDescription() const override {
        return "Flags new virtual methods without a preceding comment in classes meant to be extended: "
               "classes with user subclasses, classes named *base*, and callbacks (Checkstyle "
               "MissingJavadocMethod, Pylint C0116). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        auto classes = swe::userClasses(model);
        std::unordered_set<const Symbol*> extended;
        for (const auto* cls : classes) {
            if (const Type* base = cls->getBaseClass()) {
                if (const auto* bc = swe::asClass(*base)) {
                    extended.insert(bc);
                    if (bc->genericClass) extended.insert(bc->genericClass);
                }
            }
        }
        for (const auto* cls : classes) {
            bool isBase = extended.count(cls) || (cls->genericClass && extended.count(cls->genericClass)) ||
                          std::string_view(cls->name).find("base") != std::string_view::npos ||
                          swe::classRole(*cls) == swe::Role::Callback;
            if (!isBase) continue;
            for (const auto& member : cls->members()) {
                const SubroutineSymbol* sub = nullptr;
                if (member.kind == SymbolKind::Subroutine) sub = &member.as<SubroutineSymbol>();
                else if (member.kind == SymbolKind::MethodPrototype) sub = member.as<MethodPrototypeSymbol>().getSubroutine();
                bool pureProto = member.kind == SymbolKind::MethodPrototype &&
                                 member.as<MethodPrototypeSymbol>().flags.has(MethodFlags::Pure);
                bool isVirtual = pureProto || (sub && sub->flags.has(MethodFlags::Virtual));
                if (!isVirtual || (sub && sub->getOverride()) || isUvmStandardName(member.name)) continue;
                if (sub && sub->flags.has(MethodFlags::BuiltIn)) continue;
                if (hasLeadingComment(member)) continue;
                diags.addViolation(
                    "SV-RDB-004", "UndocumentedExtensionPoint",
                    "Virtual method '" + std::string(cls->name) + "::" + std::string(member.name) +
                        "' is an extension point with no comment. Write down when it is called and what "
                        "an override must and must not do.",
                    core::Severity::Info, member.location);
            }
        }
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUndocumentedExtensionPoint() {
    return std::make_unique<readability::UndocumentedExtensionPoint>();
}
}
