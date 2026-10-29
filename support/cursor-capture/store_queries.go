// store_queries.go 负责调试捕获记录的查询、持久化辅助和订阅通知。
package main

import (
	"database/sql"
	"encoding/json"
	"sort"
	"time"
)

// summaries 返回指定会话的捕获摘要列表。
func (store *exchangeStore) summaries(conversationID string) ([]ExchangeSummary, error) {
	store.mu.RLock()
	defer store.mu.RUnlock()
	if store.db != nil {
		query := `SELECT payload_json, conversation_id FROM exchanges`
		arguments := []any{}
		if conversationID != "" {
			query += " WHERE conversation_id = ?"
			arguments = append(arguments, conversationID)
		}
		query += " ORDER BY started_at_ms DESC, id DESC"
		rows, err := store.db.Query(query, arguments...)
		if err != nil {
			return nil, err
		}
		defer rows.Close()
		result := make([]ExchangeSummary, 0)
		for rows.Next() {
			var payload []byte
			var persistedConversationID string
			if err := rows.Scan(&payload, &persistedConversationID); err != nil {
				return nil, err
			}
			var exchange Exchange
			if err := json.Unmarshal(payload, &exchange); err != nil {
				return nil, err
			}
			exchange.ConversationID = persistedConversationID
			if current := store.exchanges[exchange.ID]; current != nil {
				result = append(result, current.ExchangeSummary)
			} else {
				result = append(result, exchange.ExchangeSummary)
			}
		}
		return result, rows.Err()
	}
	result := make([]ExchangeSummary, 0, len(store.order))
	for _, id := range store.order {
		if exchange := store.exchanges[id]; exchange != nil {
			result = append(result, exchange.ExchangeSummary)
		}
	}
	return result, nil
}

// get 返回内存或数据库中的完整捕获副本。
func (store *exchangeStore) get(id string) (Exchange, bool, error) {
	store.mu.RLock()
	defer store.mu.RUnlock()
	exchange := store.exchanges[id]
	if exchange != nil {
		return cloneExchange(*exchange), true, nil
	}
	if store.db == nil {
		return Exchange{}, false, nil
	}
	persisted, err := store.loadPersistedLocked(id)
	if err != nil {
		return Exchange{}, false, err
	}
	if persisted == nil {
		return Exchange{}, false, nil
	}
	return *persisted, true, nil
}

// loadPersistedLocked 从 SQLite 读取单条捕获并在必要时解码回填。
func (store *exchangeStore) loadPersistedLocked(id string) (*Exchange, error) {
	var payload []byte
	var conversationID string
	err := store.db.QueryRow("SELECT payload_json, conversation_id FROM exchanges WHERE id = ?", id).Scan(&payload, &conversationID)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var persisted Exchange
	if err := json.Unmarshal(payload, &persisted); err != nil {
		return nil, err
	}
	persisted.ConversationID = conversationID
	return &persisted, nil
}

// clear 清除数据库、内存索引和会话关联。
func (store *exchangeStore) clear() error {
	store.mu.Lock()
	var err error
	if store.db != nil {
		_, err = store.db.Exec("DELETE FROM exchanges")
		if err != nil {
			store.lastError = err.Error()
		}
	}
	if err == nil {
		store.order = nil
		store.exchanges = make(map[string]*Exchange)
		store.lastError = ""
	}
	store.mu.Unlock()
	if err == nil {
		store.publish(storeEvent{Type: "cleared"})
	}
	return err
}

// conversations 按会话聚合持久化捕获统计。
func (store *exchangeStore) conversations() ([]ConversationSummary, error) {
	store.mu.RLock()
	defer store.mu.RUnlock()
	if store.db == nil {
		groups := make(map[string]*ConversationSummary)
		for _, exchange := range store.exchanges {
			group := groups[exchange.ConversationID]
			if group == nil {
				group = &ConversationSummary{ConversationID: exchange.ConversationID}
				groups[exchange.ConversationID] = group
			}
			group.ExchangeCount++
			group.RequestBytes += exchange.RequestBytes
			group.ResponseBytes += exchange.ResponseBytes
			if exchange.StartedAt.After(group.LastStartedAt) {
				group.LastStartedAt = exchange.StartedAt
			}
		}
		result := make([]ConversationSummary, 0, len(groups))
		for _, group := range groups {
			result = append(result, *group)
		}
		sort.Slice(result, func(i, j int) bool { return result[i].LastStartedAt.After(result[j].LastStartedAt) })
		return result, nil
	}
	rows, err := store.db.Query(`SELECT conversation_id, COUNT(*), MAX(started_at_ms),
		COALESCE(SUM(request_bytes), 0), COALESCE(SUM(response_bytes), 0)
		FROM exchanges GROUP BY conversation_id ORDER BY MAX(started_at_ms) DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := make([]ConversationSummary, 0)
	for rows.Next() {
		var summary ConversationSummary
		var startedAtMS int64
		if err := rows.Scan(&summary.ConversationID, &summary.ExchangeCount, &startedAtMS, &summary.RequestBytes, &summary.ResponseBytes); err != nil {
			return nil, err
		}
		summary.LastStartedAt = time.UnixMilli(startedAtMS)
		result = append(result, summary)
	}
	return result, rows.Err()
}

// persistLocked 将当前捕获快照写入 SQLite。
func (store *exchangeStore) persistLocked(exchange *Exchange) {
	if store.db == nil || exchange == nil {
		return
	}
	payload, err := json.Marshal(exchange)
	if err == nil {
		_, err = store.db.Exec(`INSERT INTO exchanges (
			id, started_at_ms, conversation_id, request_id, state, request_bytes,
			response_bytes, payload_json, updated_at_ms
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT(id) DO UPDATE SET
			started_at_ms = excluded.started_at_ms,
			conversation_id = excluded.conversation_id,
			request_id = excluded.request_id,
			state = excluded.state,
			request_bytes = excluded.request_bytes,
			response_bytes = excluded.response_bytes,
			payload_json = excluded.payload_json,
			updated_at_ms = excluded.updated_at_ms`,
			exchange.ID, exchange.StartedAt.UnixMilli(), exchange.ConversationID,
			exchange.RequestID, exchange.State, exchange.RequestBytes,
			exchange.ResponseBytes, payload, time.Now().UnixMilli())
	}
	if err != nil {
		store.lastError = err.Error()
	} else {
		store.lastError = ""
	}
}

// associateConversationLocked 根据请求标识补齐会话关联。
func (store *exchangeStore) associateConversationLocked(exchange *Exchange) {
	if exchange.RequestID == "" {
		return
	}
	if exchange.ConversationID == "" {
		for _, candidate := range store.exchanges {
			if candidate.RequestID == exchange.RequestID && candidate.ConversationID != "" {
				exchange.ConversationID = candidate.ConversationID
				break
			}
		}
	}
	if exchange.ConversationID == "" && store.db != nil {
		_ = store.db.QueryRow(`SELECT conversation_id FROM exchanges
			WHERE request_id = ? AND conversation_id != ''
			ORDER BY started_at_ms DESC LIMIT 1`, exchange.RequestID).Scan(&exchange.ConversationID)
	}
	if exchange.ConversationID == "" {
		return
	}
	for _, candidate := range store.exchanges {
		if candidate.RequestID == exchange.RequestID && candidate.ConversationID == "" {
			candidate.ConversationID = exchange.ConversationID
			store.persistLocked(candidate)
		}
	}
	if store.db != nil {
		if _, err := store.db.Exec(`UPDATE exchanges SET conversation_id = ?, updated_at_ms = ?
			WHERE request_id = ? AND conversation_id = ''`, exchange.ConversationID, time.Now().UnixMilli(), exchange.RequestID); err != nil {
			store.lastError = err.Error()
		}
	}
}

// maxNumericID 返回数据库中已使用的最大数字捕获编号。
func (store *exchangeStore) maxNumericID() uint64 {
	store.mu.RLock()
	defer store.mu.RUnlock()
	var maximum uint64
	if store.db != nil {
		_ = store.db.QueryRow("SELECT COALESCE(MAX(CAST(id AS INTEGER)), 0) FROM exchanges").Scan(&maximum)
	}
	return maximum
}

// close 关闭数据库连接并终止后续订阅通知。
func (store *exchangeStore) close() error {
	store.mu.Lock()
	defer store.mu.Unlock()
	if store.db == nil {
		return nil
	}
	err := store.db.Close()
	store.db = nil
	return err
}

// status 返回数据库路径和最近一次数据库错误。
func (store *exchangeStore) status() (string, string) {
	store.mu.RLock()
	defer store.mu.RUnlock()
	return store.databasePath, store.lastError
}

// subscribe 注册一个捕获变化订阅者。
func (store *exchangeStore) subscribe() (<-chan storeEvent, func()) {
	updates := make(chan storeEvent, 32)
	store.mu.Lock()
	store.subscribers[updates] = struct{}{}
	store.mu.Unlock()
	return updates, func() {
		store.mu.Lock()
		if _, ok := store.subscribers[updates]; ok {
			delete(store.subscribers, updates)
			close(updates)
		}
		store.mu.Unlock()
	}
}

// publish 非阻塞地广播捕获变化事件。
func (store *exchangeStore) publish(event storeEvent) {
	store.mu.RLock()
	defer store.mu.RUnlock()
	for subscriber := range store.subscribers {
		select {
		case subscriber <- event:
		default:
		}
	}
}

// cloneExchange 深拷贝捕获及其请求响应载荷。
func cloneExchange(exchange Exchange) Exchange {
	exchange.Request = clonePayload(exchange.Request)
	exchange.Response = clonePayload(exchange.Response)
	return exchange
}

// clonePayload 深拷贝头信息和 Connect 帧切片。
func clonePayload(payload Payload) Payload {
	payload.Headers = append([]Header(nil), payload.Headers...)
	payload.Frames = append([]FrameView(nil), payload.Frames...)
	return payload
}

// elapsedMS 计算从开始时间到当前时间的毫秒耗时。
func elapsedMS(startedAt time.Time) int64 {
	if startedAt.IsZero() {
		return 0
	}
	return time.Since(startedAt).Milliseconds()
}

// sortedHeaders 生成脱敏且按名称排序的请求头列表。
func sortedHeaders(headers map[string][]string) []Header {
	result := make([]Header, 0, len(headers))
	for name, values := range headers {
		value := ""
		for index, item := range values {
			if index > 0 {
				value += ", "
			}
			value += item
		}
		if isSensitiveHeader(name) && value != "" {
			value = "[已隐藏]"
		}
		result = append(result, Header{Name: name, Value: value})
	}
	sort.Slice(result, func(left, right int) bool {
		return result[left].Name < result[right].Name
	})
	return result
}

// isSensitiveHeader 判断请求头是否包含鉴权或隐私信息。
func isSensitiveHeader(name string) bool {
	switch httpCanonicalLower(name) {
	case "authorization", "cookie", "set-cookie", "proxy-authorization", "x-api-key":
		return true
	default:
		return false
	}
}

// httpCanonicalLower 将请求头名称规范化为小写形式。
func httpCanonicalLower(value string) string {
	buffer := make([]byte, len(value))
	for index := range value {
		character := value[index]
		if character >= 'A' && character <= 'Z' {
			character += 'a' - 'A'
		}
		buffer[index] = character
	}
	return string(buffer)
}
