#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::uvm {

class EmptySequenceBodyVisitor : public slang::syntax::SyntaxVisitor<EmptySequenceBodyVisitor> {
public:
    explicit EmptySequenceBodyVisitor(core::DiagnosticEngine& diags, const uvmcheck::uvm::UvmModel& uvmModel)
        : diags(diags), uvmModel(uvmModel) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& clsNode) {
        std::string prevClass = currentClass;
        bool prevIsSeq = currentIsSeq;

        currentClass = clsNode.name.valueText().empty() ? "" : std::string(clsNode.name.valueText());
        currentIsSeq = false;

        if (!currentClass.empty() && currentClass != "uvm_sequence" && currentClass != "uvm_sequence_base") {
            for (const auto* cls : uvmModel.getAllClasses()) {
                if (cls && cls->name == currentClass) {
                    if (uvmModel.isUvmSequence(*cls) || uvmModel.isDerivedFrom(*cls, "uvm_sequence_base")) {
                        currentIsSeq = true;
                    }
                    break;
                }
            }

            if (!currentIsSeq && clsNode.extendsClause && clsNode.extendsClause->baseName) {
                std::string baseName = std::string(clsNode.extendsClause->baseName->toString());
                if (baseName.find("uvm_sequence") != std::string::npos) {
                    currentIsSeq = true;
                }
            }
        }

        visitDefault(clsNode);

        currentClass = prevClass;
        currentIsSeq = prevIsSeq;
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (currentIsSeq && func.prototype && func.prototype->name) {
            std::string taskName = std::string(func.prototype->name->toString());
            taskName.erase(0, taskName.find_first_not_of(" \t\n\r"));
            if (size_t end = taskName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                taskName.erase(end + 1);
            }

            if (taskName == "body" && func.prototype->keyword.kind == slang::parsing::TokenKind::TaskKeyword) {
                bool hasMeaningfulStmt = false;
                for (const auto* item : func.items) {
                    if (!item) continue;
                    if (item->kind != slang::syntax::SyntaxKind::EmptyStatement) {
                        hasMeaningfulStmt = true;
                        break;
                    }
                }

                if (!hasMeaningfulStmt) {
                    diags.addViolation(
                        "UVM-SEQ-005",
                        "EmptySequenceBody",
                        "Sequence class '" + currentClass +
                        "' contains an empty 'body()' task. Empty sequences produce no stimulus transactions and typically indicate incomplete VIP or obsolete test scaffolding.",
                        core::Severity::Warning,
                        func.prototype->name->sourceRange().start(),
                        {}
                    );
                }
            }
        }
        visitDefault(func);
    }

private:
    core::DiagnosticEngine& diags;
    const uvmcheck::uvm::UvmModel& uvmModel;
    std::string currentClass;
    bool currentIsSeq = false;
};

class EmptySequenceBody : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-005"; }
    std::string_view getName() const override { return "EmptySequenceBody"; }
    std::string_view getDescription() const override {
        return "Detects sequence classes whose body() task contains no executable statements. "
               "Empty sequence bodies generate zero stimulus transactions.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        EmptySequenceBodyVisitor visitor(diags, uvmModel);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

namespace {

using namespace slang::ast;

bool sameClass(const Type& type, const ClassType& target) {
    const ClassType* held = swe::asClass(type);
    return held && &held->getCanonicalType() == &target.getCanonicalType();
}

bool mentionedFrom(const ClassType& target, const ClassType& host) {
    if (&host.getCanonicalType() == &target.getCanonicalType()) return false;
    if (swe::derivesFrom(host, target)) return true;
    for (const auto& member : host.members()) {
        if (member.kind == SymbolKind::ClassProperty) {
            if (sameClass(member.as<ClassPropertySymbol>().getType(), target)) return true;
        } else if (member.kind == SymbolKind::Subroutine) {
            const auto& sub = member.as<SubroutineSymbol>();
            if (sameClass(sub.getReturnType(), target)) return true;
            for (const auto* arg : sub.getArguments()) {
                if (sameClass(arg->getType(), target)) return true;
            }
            bool hit = false;
            swe::forEachCall(sub.getBody(), [&](const CallExpression& call) {
                if (call.type && sameClass(*call.type, target)) hit = true;
            });
            struct Locals : public ASTVisitor<Locals, VisitFlags::AllGood> {
                const ClassType* target = nullptr;
                bool hit = false;
                void handle(const VariableSymbol& v) {
                    if (target && sameClass(v.getType(), *target)) hit = true;
                    visitDefault(v);
                }
            } locals;
            locals.target = &target;
            sub.getBody().visit(locals);
            if (hit || locals.hit) return true;
        }
    }
    return false;
}

bool mentionedAnywhere(const ClassType& target, const ::uvmcheck::uvm::UvmModel& model) {
    for (const auto* cls : swe::userClasses(model)) {
        if (mentionedFrom(target, *cls)) return true;
    }
    return false;
}

bool compilationHasTest(const ::uvmcheck::uvm::UvmModel& model) {
    for (const auto* cls : swe::userClasses(model)) {
        if (swe::classRole(*cls) == swe::Role::Test) return true;
    }
    return false;
}

} // namespace

class UnusedBenchType : public Rule {
public:
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }
    RuleMaturity getMaturity() const override { return RuleMaturity::Experimental; }

protected:
    void report(const ::uvmcheck::uvm::UvmModel& model, core::DiagnosticEngine& diags, swe::Role role,
                std::string_view id, std::string_view name, std::string_view noun) {
        if (!compilationHasTest(model)) return;
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != role || cls->isAbstract) continue;
            if (mentionedAnywhere(*cls, model)) continue;
            diags.addViolation(
                std::string(id), std::string(name),
                std::string(noun) + " '" + std::string(cls->name) +
                    "' is never named by another class in this run, so nothing creates or starts it. "
                    "Create it from the test, or remove it. Experimental: a use in a file that was "
                    "not linted is invisible here.",
                core::Severity::Warning, cls->location);
        }
    }
};

class UnusedSequence : public UnusedBenchType {
public:
    std::string_view getId() const override { return "UVM-SEQ-014"; }
    std::string_view getName() const override { return "UnusedSequence"; }
    std::string_view getDescription() const override {
        return "Flags a sequence that no other class in the compilation names, when the run also "
               "contains a test. Experimental: the sequence may be started from a file that was not linted.";
    }
    void run(slang::ast::Compilation&, const ::uvmcheck::uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        report(model, diags, swe::Role::Sequence, "UVM-SEQ-014", "UnusedSequence", "Sequence");
    }
};

class UnusedSequenceItem : public UnusedBenchType {
public:
    std::string_view getId() const override { return "UVM-SEQ-015"; }
    std::string_view getName() const override { return "UnusedSequenceItem"; }
    std::string_view getDescription() const override {
        return "Flags a sequence item that no other class in the compilation names, when the run also "
               "contains a test. Experimental: the item may be created from a file that was not linted.";
    }
    void run(slang::ast::Compilation&, const ::uvmcheck::uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        report(model, diags, swe::Role::Item, "UVM-SEQ-015", "UnusedSequenceItem", "Sequence item");
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptySequenceBody() {
    return std::make_unique<uvm::EmptySequenceBody>();
}
std::unique_ptr<Rule> createUnusedSequence() { return std::make_unique<uvm::UnusedSequence>(); }
std::unique_ptr<Rule> createUnusedSequenceItem() { return std::make_unique<uvm::UnusedSequenceItem>(); }
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createEmptySequenceBody)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnusedSequence)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnusedSequenceItem)
