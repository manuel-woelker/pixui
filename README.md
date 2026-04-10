# Introduction

pixui is an experimental UI framework that focuses on providing fast feedback, flexibility, and efficiency.

## Goals

* Create a UI framework that is enjoyable to use
* Enable rapid iteration on UIs
* Make it easy to write meaningful tests for UIs

## Vision

### Fast Feedback

Fast feedback loops are essential for effective UI development. By allowing the UI to be modified while the application
is running, we can quickly test and refine our ideas.

### Flexibility

The UI should be decoupled from specific implementations and instead rely on an abstraction layer that accepts drawing
commands and emits user events.

### Efficiency

The UI should minimize unnecessary work, both for developers and machines. This means only drawing what has changed.

## Implementation Ideas

### Hot Reloading, Declarative UI

The UI is defined using simple files in a custom UI modeling language. Changes to this definition update the UI in
real-time. A special design mode allows editing the UI while it's running.

### Draw Tests

Since UIs produce a list of draw commands, we can write tests for these commands or use SVG screenshots.

## Separation of Concerns

Concerns should be decoupled from each other:

### State & Logic Separation

* **Application Data Model**: The application data should exist independently of any UI.
* **Application State Transition Logic**: The logic for transitioning between application states should be independent
  of the UI and run without it.

### Presentation Separation

* **UI Layout**: The positioning of elements should be independent of their look and content.
* **Styling**: The styling of elements should be independent of their layout.
* **Text**: The text content should be independent for easier translation.

### UI Loop Separation

* **Draw Commands**: The actual drawing is decoupled by emitting a list of draw commands.
* **UI Painting**: The UI painting is decoupled from the UI loop, allowing parallel execution.
