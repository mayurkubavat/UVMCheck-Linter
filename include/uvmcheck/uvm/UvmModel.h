#pragma once

#include <cstdint>
#include <memory>
#include <mutex>
#include <string>
#include <string_view>
#include <vector>
#include <unordered_set>
#include <unordered_map>

#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/types/Type.h"
#include "uvmcheck/model/Model.h"

namespace uvmcheck::uvm {

class UvmModel {
public:
    explicit UvmModel(slang::ast::Compilation& compilation);

    /// Unique per model instance, for the lifetime of the process. Caches keyed
    /// on a design must use this, not the Compilation's address: a server that
    /// lints repeatedly can get a new Compilation at a freed one's address.
    uint64_t generation() const { return generationId; }

    /// Checks if a type is derived from a specific base class name
    bool isDerivedFrom(const slang::ast::Type& type, std::string_view baseClassName) const;

    /// Checks if a type is derived from uvm_component
    bool isUvmComponent(const slang::ast::Type& type) const {
        return isDerivedFrom(type, "uvm_component");
    }

    /// Checks if a type is derived from uvm_object
    bool isUvmObject(const slang::ast::Type& type) const {
        return isDerivedFrom(type, "uvm_object");
    }

    /// Checks if a type is derived from uvm_sequence_item
    bool isUvmSequenceItem(const slang::ast::Type& type) const {
        return isDerivedFrom(type, "uvm_sequence_item");
    }

    /// Checks if a type is derived from uvm_sequence_base
    bool isUvmSequence(const slang::ast::Type& type) const {
        return isDerivedFrom(type, "uvm_sequence_base") || isDerivedFrom(type, "uvm_sequence");
    }

    /// Returns all class types discovered in compilation
    const std::vector<const slang::ast::ClassType*>& getAllClasses() const {
        return discoveredClasses;
    }

    /// Returns true if the method name is a recognized standard UVM phase
    static bool isStandardUvmPhase(std::string_view name);

    /// Returns true if the phase is a function phase (cannot consume simulation time)
    static bool isFunctionPhase(std::string_view name);

    /// Returns true if the phase is a task phase (run_phase, etc.)
    static bool isTaskPhase(std::string_view name);

    /// Returns all static component hierarchical paths in the verification environment
    const std::vector<std::string>& getComponentPaths() const {
        return componentPaths;
    }

    /// Checks if a UVM glob string (e.g. "*.m_env.m_agent*") matches any discovered component path
    bool matchesAnyComponentPath(std::string_view globPattern) const;

    /// The slang-free snapshot of this design (uvmcheck::model), built on
    /// first use and shared by every reader. New rules should read this rather
    /// than slang's AST, so they keep working when the front end changes.
    const model::Model& facade() const;

    /// Dumps the UVM semantic model (classes, methods, config_db, TLM) as JSON for Python SDK
    void dumpSemanticJson(std::ostream& out) const;

private:
    uint64_t generationId = 0;
    slang::ast::Compilation& compilation;
    std::vector<const slang::ast::ClassType*> discoveredClasses;
    std::vector<std::string> componentPaths;

    struct FacadeCache {
        std::once_flag once;
        std::unique_ptr<model::Model> model;
    };
    std::unique_ptr<FacadeCache> facadeCache = std::make_unique<FacadeCache>();

    void indexScope(const slang::ast::Scope& scope);
    void buildComponentDag();
};

} // namespace uvmcheck::uvm
