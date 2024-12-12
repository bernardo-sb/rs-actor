# Implementing Actors with Tokio: A Practical Guide

This repo support the blog post: [The Actor Model in Rust](http://bernardo.shippedbrain.com/rust_actor/)

When designing systems for high concurrency and scalability, the actor model is a popular choice. It encapsulates state and behavior, communicating solely through message passing. Rust's async ecosystem, powered by **Tokio**, is a powerful framework for implementing this model. In this post, we’ll dive deep into creating an actor-based system in Rust, providing a detailed walkthrough of an implementation.

## What is the Actor Model?

The actor model treats "actors" as the primary unit of computation:
- **Encapsulation**: Each actor manages its state and behavior.
- **Asynchronous Message Passing**: Actors communicate through messages, avoiding shared mutable state.
- **Concurrency**: Each actor runs independently, providing natural parallelism.

This model is an interesting exercise for Rust due to its emphasis on safety and a great fit due to its concurrency and performance.
