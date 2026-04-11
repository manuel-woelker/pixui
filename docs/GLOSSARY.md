# Glossary

## Component

A family of visible UI objects, that share function, data shape and drawing, e.g. Button, TextInput, Label, etc.

### Primitive Components

Primitive Components are leaf components drawn via draw commands.

### Composite Components

Composite Components are components that contain other components, e.g. a form, a list, etc.

## Element

Elements are concrete occurrences of components in a UI, e.g. an "OK" button, a name label.

### Component Elements

Component elements are elements that correspond to a component, e.g. a button

### Control Elements

Control elements are elements that control how elements are rendered, e.g. for conditional rendering (`match`) or repetition (`for each`)

## Element Instance

A concrete instance of an element with its own state, properties, and positioning.

## Element Tree

Elements form a tree with a Root element, which defines the layout and content of a window or dialog.

These trees occur in various stages:

### AstTree

The tree of elements as parsed from its textual representation.

### IrTree

An intermediate representation of the element tree, with names resolved to indices.

### InstanceTree

A tree of element instances, along with their associated data.

