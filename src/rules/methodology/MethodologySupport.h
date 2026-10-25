#pragma once

// Helpers shared by the methodology-guide rules (UVM-RUS, UVM-ROL, UVM-CPL, UVM-PAT,
// RAL-ADP, UVM-REP-007/008): who creates what, config_db calls, class properties by role,
// and the failure branch of a check. Built on SweSupport's role classification.

#include <functional>
#include <string>
#include <string_view>
#include <vector>

#include "slang/ast/Expression.h"
#include "slang/ast/Compilation.h"
#include "slang/ast/Statement.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/text/SourceManager.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::meth {

/// String literal arguments ("vif"), looking through conversions.
bool isStringLiteral(const slang::ast::Expression& e);
std::string literalValue(const slang::ast::Expression& e);
bool isNullLiteral(const slang::ast::Expression& e);

/// A buffer of the UVM library or the test stub: it, or a file that includes it, is named
/// uvm_*. Rules that read source text or syntax use this where symbol-based rules use
/// swe::isLibrarySymbol.
bool isLibraryBuffer(const slang::SourceManager& sm, slang::BufferID buffer);

/// A syntax tree all of whose source files are library buffers (uvm_pkg.sv and its includes).
bool isLibraryTree(const slang::SourceManager& sm, const slang::syntax::SyntaxTree& tree);

/// Syntax trees that hold user code (a test file still includes the UVM stub, so callers
/// also check each node's own buffer).
std::vector<const slang::syntax::SyntaxTree*> userSyntaxTrees(slang::ast::Compilation& comp);

/// The tokens of a syntax node as the user wrote them, for clone detection: a macro expansion
/// becomes one `name token followed by its arguments, so `uvm_info(...) counts as the call
/// and not as the code it expands to. Identifiers are marked so that callers can rename them.
struct NormToken {
    std::string text;
    bool identifier = false;
};
void writtenTokens(const slang::SourceManager& sm, const slang::syntax::SyntaxNode& node,
                   std::vector<NormToken>& out);

/// One string for a token sequence. With `rename`, identifiers become $1, $2 ... in order of
/// first use and literals become #, so a copy with renamed variables still matches. A name
/// followed by "(" is a call and keeps its text.
std::string tokenKey(const std::vector<NormToken>& tokens, size_t begin, size_t end, bool rename);

/// The file says in its header comment that a tool generated it ("auto-generated", "do not
/// edit"). Its copies come from a template, so the duplication rules leave it out.
bool isGeneratedBuffer(const slang::SourceManager& sm, slang::BufferID buffer);

/// A location in user code, after resolving macro expansions to where they were written.
bool isUserLocation(const slang::SourceManager& sm, slang::SourceLocation loc);

/// The method overrides a virtual method of a base class (user or library).
bool overridesBaseMethod(const slang::ast::SubroutineSymbol& sub);

/// Name of the class that declares a call's subroutine, without parameters
/// ("uvm_config_db" for uvm_config_db#(int)::get), or "" for free and system calls.
std::string_view declaringClassName(const slang::ast::CallExpression& call);

/// T::type_id::create(...) through uvm_component_registry / uvm_object_registry.
bool isFactoryCreate(const slang::ast::CallExpression& call);

/// uvm_config_db#(T)::<method>(...) (method is "get" or "set").
bool isConfigDbCall(const slang::ast::CallExpression& call, std::string_view method);

/// Role of a class type, looking through unpacked arrays and queues of handles.
swe::Role roleOfType(const slang::ast::Type& type);

/// The class type, looking through unpacked arrays and queues of handles.
const slang::ast::ClassType* classOfType(const slang::ast::Type& type);

/// Class name without parameters.
std::string_view plainName(const slang::ast::ClassType& cls);

/// Properties declared in this class (not inherited).
std::vector<const slang::ast::ClassPropertySymbol*> ownProperties(const slang::ast::ClassType& cls);

/// One object creation: T::type_id::create(...) or new.
struct Creation {
    const slang::ast::ClassType* type = nullptr;
    slang::SourceLocation loc;
    /// True when an enclosing condition, or an earlier early return, tests the agent's
    /// active/passive setting (is_active, get_is_active(), UVM_ACTIVE, cfg.active ...).
    bool guardedByActive = false;
};

/// Every creation inside a statement, in source order.
std::vector<Creation> creationsIn(const slang::ast::Statement& stmt);

/// Every creation in the methods of a class.
std::vector<Creation> creationsIn(const slang::ast::ClassType& cls);

/// True when a condition's text tests the active/passive setting.
bool mentionsActive(const slang::ast::Expression& cond);

/// A conditional whose branch runs when a check fails: a failed compare(), or an
/// expected-versus-actual mismatch. `what` describes the check for messages.
struct FailureBranch {
    const slang::ast::Statement* branch = nullptr;
    const slang::ast::Expression* condition = nullptr;
    std::string what;
};

/// Failure branches of checks anywhere inside a statement.
std::vector<FailureBranch> failureBranches(const slang::ast::Statement& stmt);

} // namespace uvmcheck::rules::meth
