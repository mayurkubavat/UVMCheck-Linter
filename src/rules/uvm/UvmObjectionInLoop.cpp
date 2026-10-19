#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/statements/LoopStatements.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

namespace uvmcheck::rules::uvm {

class UvmObjectionInLoopVisitor
    : public slang::ast::ASTVisitor<UvmObjectionInLoopVisitor,
                                    slang::ast::VisitFlags::AllGood> {
public:
  UvmObjectionInLoopVisitor(core::DiagnosticEngine &diags) : diags(diags) {}

  void handle(const slang::ast::ForLoopStatement &stmt) {
    loopDepth++;
    visitDefault(stmt);
    loopDepth--;
  }

  void handle(const slang::ast::RepeatLoopStatement &stmt) {
    loopDepth++;
    visitDefault(stmt);
    loopDepth--;
  }

  void handle(const slang::ast::ForeachLoopStatement &stmt) {
    loopDepth++;
    visitDefault(stmt);
    loopDepth--;
  }

  void handle(const slang::ast::WhileLoopStatement &stmt) {
    loopDepth++;
    visitDefault(stmt);
    loopDepth--;
  }

  void handle(const slang::ast::DoWhileLoopStatement &stmt) {
    loopDepth++;
    visitDefault(stmt);
    loopDepth--;
  }

  void handle(const slang::ast::ForeverLoopStatement &stmt) {
    loopDepth++;
    visitDefault(stmt);
    loopDepth--;
  }

  void handle(const slang::ast::CallExpression &expr) {
    if (loopDepth > 0) {
      std::string_view name = expr.getSubroutineName();
      if (name == "raise_objection" || name == "drop_objection") {
        diags.addViolation(
            "UVM-OBJ-002", "UvmObjectionInLoop",
            "UVM objection method '" + std::string(name) +
                "' invoked inside a loop. Raising or dropping objections per "
                "iteration "
                "causes severe simulation performance degradation. Raise the "
                "objection once "
                "before the loop and drop it after the loop completes.",
            core::Severity::Warning, expr.sourceRange.start());
      }
    }
    visitDefault(expr);
  }

private:
  core::DiagnosticEngine &diags;
  int loopDepth = 0;
};

class UvmObjectionInLoop : public Rule {
public:
  std::string_view getId() const override { return "UVM-OBJ-002"; }
  std::string_view getName() const override { return "UvmObjectionInLoop"; }
  std::string_view getDescription() const override {
    return "Avoid raising and dropping objections inside high-frequency "
           "transaction loops "
           "to prevent severe simulation performance degradation.";
  }
  std::string_view getCategory() const override { return "UVM"; }
  core::Severity getDefaultSeverity() const override {
    return core::Severity::Warning;
  }

  void run(slang::ast::Compilation &compilation,
           const uvmcheck::uvm::UvmModel & /*uvmModel*/,
           core::DiagnosticEngine &diags) override {
    UvmObjectionInLoopVisitor visitor(diags);
    compilation.getRoot().visit(visitor);
  }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmObjectionInLoop() {
  return std::make_unique<uvm::UvmObjectionInLoop>();
}
} // namespace uvmcheck::rules
