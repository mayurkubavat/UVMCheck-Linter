#pragma once

// Shared helpers for the software-engineering rule families (SV-BUG, SV-ERR, SV-DSN,
// SV-MET, SV-API, SV-RDB, SV-RSC, UVM-ARC, UVM-TST ...). They classify UVM classes by
// role, enumerate user (non-library) classes and methods, normalize source text and
// recognize UVM reporting calls, so each rule file only holds its own check.

#include <cstdint>
#include <functional>
#include <string>
#include <string_view>
#include <unordered_set>
#include <vector>

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Compilation.h"
#include "slang/ast/Expression.h"
#include "slang/ast/Statement.h"
#include "slang/ast/TimingControl.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::rules::swe {

/// What a class is for in a UVM testbench, derived from the nearest uvm_* base class.
enum class Role {
    None,        // plain SystemVerilog class
    Object,      // uvm_object that is none of the below
    Config,      // uvm_object whose name says cfg/config
    Item,        // uvm_sequence_item
    Sequence,    // uvm_sequence / uvm_sequence_base
    Component,   // uvm_component that is none of the below
    Driver,
    Monitor,
    Sequencer,
    Agent,
    Scoreboard,
    Subscriber,
    Env,
    Test,
    RegModel,    // uvm_reg, uvm_reg_block, uvm_mem, uvm_reg_field, uvm_reg_map
    Catcher,     // uvm_report_catcher
    Callback     // uvm_callback
};

const char* roleName(Role role);
Role classRole(const slang::ast::ClassType& cls);
bool isComponentRole(Role role);

/// Name of the nearest uvm_* ancestor ("" when none).
std::string_view uvmBaseName(const slang::ast::ClassType& cls);

/// True for symbols that belong to the UVM library or the test stub, which rules must not lint.
bool isLibrarySymbol(const slang::ast::Symbol& sym);

/// User-written classes: everything UvmModel indexed plus the default specialization of
/// parameterized classes, minus library classes.
std::vector<const slang::ast::ClassType*> userClasses(const uvm::UvmModel& model);

/// Methods with bodies declared in this class (extern bodies included, built-ins excluded).
std::vector<const slang::ast::SubroutineSymbol*> classMethods(const slang::ast::ClassType& cls);

/// Free tasks and functions in packages, modules, interfaces and $unit (not class methods).
std::vector<const slang::ast::SubroutineSymbol*> freeRoutines(slang::ast::Compilation& comp);

/// Every subroutine with a body that rules should lint: class methods plus free routines.
std::vector<const slang::ast::SubroutineSymbol*> allUserRoutines(slang::ast::Compilation& comp,
                                                                 const uvm::UvmModel& model);

/// Source text with all whitespace runs collapsed to one space (and trimmed).
std::string normalizedText(const slang::syntax::SyntaxNode* node);
std::string exprText(const slang::ast::Expression& expr);
std::string stmtText(const slang::ast::Statement& stmt);

/// Location helpers that fall back to the source range start.
slang::SourceLocation locOf(const slang::ast::Statement& stmt);
slang::SourceLocation locOf(const slang::ast::Expression& expr);

/// Strips conversions and parentheses-like wrappers.
const slang::ast::Expression& stripConversions(const slang::ast::Expression& expr);

/// The variable or member symbol an lvalue/rvalue expression ultimately names, if simple.
const slang::ast::Symbol* referencedSymbol(const slang::ast::Expression& expr);

/// Class type of an expression or type, or nullptr.
const slang::ast::ClassType* asClass(const slang::ast::Type& type);

enum class ReportLevel { None, Info, Warning, Error, Fatal };

/// Recognizes uvm_report_* calls (which the `uvm_* macros expand to), $error/$fatal/$warning
/// and $display-family calls inside assertions' else branches are not reports.
ReportLevel reportLevel(const slang::ast::CallExpression& call);

/// The guard a `uvm_* macro puts around its report: if (uvm_report_enabled(...)) in the stub
/// and UVM 1.2, or if (ro.get_report_verbosity_level(...) >= V && ro.get_report_action(...) !=
/// UVM_NO_ACTION) in uvm-core 2020. Rules treat it as part of the report, not as branching.
bool isReportGuard(const slang::ast::ConditionalStatement& c);

/// uvm-core's `uvm_report_begin scaffolding: the _local_report_object_ declaration and its
/// assignment.
bool isReportScaffolding(const slang::ast::Statement& s);

/// Strongest report level issued anywhere inside a statement.
ReportLevel strongestReport(const slang::ast::Statement& stmt);

/// True when the statement always leaves the routine or stops the run:
/// return, `uvm_fatal, $fatal, $finish, or a block ending in one.
bool alwaysExits(const slang::ast::Statement& stmt);

/// Visits every call expression inside a statement.
void forEachCall(const slang::ast::Statement& stmt,
                 const std::function<void(const slang::ast::CallExpression&)>& fn);

/// Number of statements in a subroutine body, not counting blocks and lists themselves.
int countStatements(const slang::ast::Statement& stmt);

/// Per-rule numeric option from the project config (a RuleConfig attribute, or a JSON
/// rule's "options"), falling back to `defaultValue`.
int64_t ruleOption(std::string_view ruleId, std::string_view key, int64_t defaultValue);

/// True if `cls` derives (transitively) from `base`.
bool derivesFrom(const slang::ast::ClassType& cls, const slang::ast::ClassType& base);

/// Text of a call's receiver ("" when there is none), e.g. "bus_sem" for bus_sem.get().
std::string receiverText(const slang::ast::CallExpression& call);

/// Root visitor that skips the UVM library and the test stub, and also walks the default
/// specialization of parameterized classes (which slang does not visit on its own).
/// Derived visitors that declare their own handle() overloads must add
/// `using UserCodeVisitor::handle;`.
template<typename TDerived>
class UserCodeVisitor : public slang::ast::ASTVisitor<TDerived, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::ClassType& cls) {
        if (isLibrarySymbol(cls)) return;
        this->visitDefault(cls);
    }
    void handle(const slang::ast::PackageSymbol& pkg) {
        if (pkg.name == "uvm_pkg") return;
        this->visitDefault(pkg);
    }
    void handle(const slang::ast::GenericClassDefSymbol& gen) {
        if (isLibrarySymbol(gen)) return;
        if (const auto* spec = gen.getDefaultSpecialization(*gen.getParentScope())) {
            if (spec->isClass()) spec->getCanonicalType().template as<slang::ast::ClassType>().visit(
                static_cast<TDerived&>(*this));
        }
    }
};

/// True for `#<constant>` delays: literals, parameters and constant expressions of them.
bool isFixedDelay(const slang::ast::TimingControl& timing);

/// True when the expression is a compile-time constant (literal, parameter, folded expression).
bool isConstantExpr(const slang::ast::Expression& expr);

/// The statement's text as written in the source file (macro calls unexpanded), with
/// comments removed and whitespace collapsed. Falls back to stmtText().
std::string originalText(const slang::ast::Compilation& comp, const slang::ast::Statement& stmt);

/// Number of arguments of a call written positionally, or -1 when any argument is named
/// (.addr(x)) or the call has no argument list syntax.
int positionalArgCount(const slang::ast::CallExpression& call);

/// The variable at the root of an lvalue such as q[i], s.f or a[1].b ("q", "s", "a").
const slang::ast::Symbol* rootSymbol(const slang::ast::Expression& expr);

/// Variables written anywhere in a statement: assigned, incremented, passed to output or
/// non-const ref arguments, filled by $cast/$sscanf/randomize, and (when containerMethods)
/// changed by queue/array methods such as push_back and delete. Roots of selects are reported
/// (q for q[i] = x, s for s.f = x).
std::unordered_set<const slang::ast::Symbol*> writtenSymbols(const slang::ast::Statement& stmt,
                                                             bool containerMethods = true);

/// Variables and members named anywhere in an expression.
std::unordered_set<const slang::ast::Symbol*> referencedSymbols(const slang::ast::Expression& expr);

/// sem.get() / sem.try_get() on a built-in semaphore.
bool isSemaphoreGet(const slang::ast::CallExpression& call);

/// 1-based line of a location, for messages that point at a second place.
size_t lineOf(const slang::ast::Compilation& comp, slang::SourceLocation loc);

} // namespace uvmcheck::rules::swe
