#pragma once

#include <coroutine>
#include <exception>
#include <utility>

namespace uvmcheck::engine {

/// Coroutine returned by Rule::check().
///
/// A rule's check() body runs top to bottom like a normal function. When it
/// reaches `co_await ctx.visitSyntax(v)` or `co_await ctx.visitAst(v)` it is
/// suspended, the engine walks the design once for every rule waiting on that
/// walk, and the rule resumes afterwards to do any post-walk reporting.
class RuleTask {
public:
    struct promise_type {
        std::exception_ptr exception;

        RuleTask get_return_object() {
            return RuleTask(std::coroutine_handle<promise_type>::from_promise(*this));
        }
        std::suspend_always initial_suspend() noexcept { return {}; }
        std::suspend_always final_suspend() noexcept { return {}; }
        void return_void() noexcept {}
        void unhandled_exception() noexcept { exception = std::current_exception(); }
    };

    RuleTask() = default;
    explicit RuleTask(std::coroutine_handle<promise_type> h) : handle(h) {}
    RuleTask(RuleTask&& other) noexcept : handle(std::exchange(other.handle, {})) {}
    RuleTask& operator=(RuleTask&& other) noexcept {
        if (this != &other) {
            destroy();
            handle = std::exchange(other.handle, {});
        }
        return *this;
    }
    RuleTask(const RuleTask&) = delete;
    RuleTask& operator=(const RuleTask&) = delete;
    ~RuleTask() { destroy(); }

    bool valid() const { return static_cast<bool>(handle); }
    bool done() const { return !handle || handle.done(); }

    /// Runs the rule until its next suspension point (or completion).
    /// Exceptions thrown by the rule body are rethrown here.
    void resume() {
        if (done()) return;
        handle.resume();
        if (handle.done() && handle.promise().exception) {
            std::rethrow_exception(handle.promise().exception);
        }
    }

private:
    void destroy() {
        if (handle) {
            handle.destroy();
            handle = {};
        }
    }

    std::coroutine_handle<promise_type> handle;
};

} // namespace uvmcheck::engine
