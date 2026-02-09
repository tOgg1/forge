package data

import (
	"container/list"
	"sync"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type timedEntry[T any] struct {
	value   T
	expires time.Time
	ok      bool
}

type messageCache struct {
	mu       sync.Mutex
	capacity int
	order    *list.List
	entries  map[string]*list.Element
}

type messageCacheEntry struct {
	key     string
	modTime time.Time
	message fmail.Message
}

func newMessageCache(capacity int) *messageCache {
	if capacity <= 0 {
		capacity = defaultMessageCacheSize
	}
	return &messageCache{
		capacity: capacity,
		order:    list.New(),
		entries:  make(map[string]*list.Element, capacity),
	}
}

func (c *messageCache) get(key string, modTime time.Time) (fmail.Message, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()

	elem, ok := c.entries[key]
	if !ok {
		return fmail.Message{}, false
	}
	entry := elem.Value.(*messageCacheEntry)
	if !entry.modTime.Equal(modTime) {
		c.order.Remove(elem)
		delete(c.entries, key)
		return fmail.Message{}, false
	}
	c.order.MoveToFront(elem)
	return cloneMessage(entry.message), true
}

func (c *messageCache) put(key string, modTime time.Time, message fmail.Message) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if elem, ok := c.entries[key]; ok {
		entry := elem.Value.(*messageCacheEntry)
		entry.modTime = modTime
		entry.message = cloneMessage(message)
		c.order.MoveToFront(elem)
		return
	}

	entry := &messageCacheEntry{
		key:     key,
		modTime: modTime,
		message: cloneMessage(message),
	}
	elem := c.order.PushFront(entry)
	c.entries[key] = elem

	for c.order.Len() > c.capacity {
		last := c.order.Back()
		if last == nil {
			break
		}
		c.order.Remove(last)
		evicted := last.Value.(*messageCacheEntry)
		delete(c.entries, evicted.key)
	}
}
