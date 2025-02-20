`store.js` holds the main datastructure for the application that is serialized to stored in the json file.

However, this may not be the most efficient data structure for runtime use; therefore, it is converted into more optimized structures when the app is running.

`SlotMapStore`
This structure holds all the workspaces and to-dos in a `SlotMap`, facilitating easier manipulation of the tree structure without excessive effort against the borrow checker. It represents the primary data structure, which is modified, read, and later converted to a `Store` type for writing back to the JSON file upon application closure.

There is also `SlotTreeState`, which stores the state of the trees rendered in the app UI. The tree data structure holds references to keys of the workspaces and to-dos, along with additional properties such as `parent` and `depth`. These properties prove useful when implementing actions like moving an item down in the UI or focusing on the previous item.
