`store.js` holds the main datastructure for the application that is serialized to stored in the json file.

But this isnt the most efficient data structure when using the app. So these are then converted from and into these structures when the app is running.   

`SlotMapStore`
This holds all the workspaces and todos in a `SlotMap`. This makes it easier to working with the tree structure without wrestling too much with the borrow checker. This is the main datastructure, this is what is modified and read and what is converted to a `Store` type and written to the json file when the app is closed.

There is also `SlotTreeState` which stores the state of the trees rendered in the app ui. The tree datastructure holds references to keys of the workspaces and todos and also some extra properties like `parent` ,`depth` etc. This comes in handy when you want to say `move item down the ui` or `focus previous item` etc. 