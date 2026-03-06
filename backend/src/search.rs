use tantivy::{
    collector::{Count, TopDocs},
    directory::MmapDirectory,
    query::QueryParser,
    schema::*,
    Index, IndexReader, IndexWriter,
};
use std::sync::Arc;
use pinyin::ToPinyin;

use crate::error::AppError;

pub struct PatientIndex {
    index: Index,
    reader: IndexReader,
    writer: Arc<tokio::sync::Mutex<IndexWriter>>,
    id_field: Field,
    name_field: Field,
    phone_field: Field,
    pinyin_field: Field,
    notes_field: Field,
}

impl PatientIndex {
    pub fn new(index_dir: &str) -> Result<Self, AppError> {
        std::fs::create_dir_all(index_dir)
            .map_err(|e| AppError::internal(format!("创建索引目录失败: {}", e)))?;

        let mut schema_builder = Schema::builder();
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let name_field = schema_builder.add_text_field("name", TEXT);
        let phone_field = schema_builder.add_text_field("phone", TEXT);
        let pinyin_field = schema_builder.add_text_field("pinyin", TEXT);
        let notes_field = schema_builder.add_text_field("notes", TEXT);
        let schema = schema_builder.build();

        let dir = MmapDirectory::open(index_dir)
            .map_err(|e| AppError::internal(format!("打开索引目录失败: {}", e)))?;
        let index = Index::open_or_create(dir, schema.clone())
            .map_err(|e| AppError::internal(format!("创建索引失败: {}", e)))?;
        let writer = index
            .writer(50_000_000)
            .map_err(|e| AppError::internal(format!("创建索引写入器失败: {}", e)))?;
        let reader = index
            .reader()
            .map_err(|e| AppError::internal(format!("创建索引读取器失败: {}", e)))?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(tokio::sync::Mutex::new(writer)),
            id_field,
            name_field,
            phone_field,
            pinyin_field,
            notes_field,
        })
    }

    /// Sync version for use during startup (index rebuild).
    pub fn add_or_update_sync(&self, patient_id: &str, name: &str, phone: &str, notes: &str) -> Result<(), AppError> {
        let id_term = tantivy::Term::from_field_text(self.id_field, patient_id);
        let writer = self.writer.blocking_lock();
        writer.delete_term(id_term);

        let pinyin_text = build_pinyin_text(name);

        let mut doc = TantivyDocument::default();
        doc.add_text(self.id_field, patient_id);
        doc.add_text(self.name_field, name);
        doc.add_text(self.phone_field, phone);
        doc.add_text(self.pinyin_field, &pinyin_text);
        doc.add_text(self.notes_field, notes);
        writer.add_document(doc)
            .map_err(|e| AppError::internal(format!("添加索引文档失败: {}", e)))?;
        Ok(())
    }

    pub async fn add_or_update(&self, patient_id: &str, name: &str, phone: &str, notes: &str) -> Result<(), AppError> {
        let id_term = tantivy::Term::from_field_text(self.id_field, patient_id);
        let writer = self.writer.lock().await;
        writer.delete_term(id_term);

        let pinyin_text = build_pinyin_text(name);

        let mut doc = TantivyDocument::default();
        doc.add_text(self.id_field, patient_id);
        doc.add_text(self.name_field, name);
        doc.add_text(self.phone_field, phone);
        doc.add_text(self.pinyin_field, &pinyin_text);
        doc.add_text(self.notes_field, notes);
        writer.add_document(doc)
            .map_err(|e| AppError::internal(format!("添加索引文档失败: {}", e)))?;
        Ok(())
    }

    pub async fn delete(&self, patient_id: &str) -> Result<(), AppError> {
        let id_term = tantivy::Term::from_field_text(self.id_field, patient_id);
        let writer = self.writer.lock().await;
        writer.delete_term(id_term);
        Ok(())
    }

    pub fn search_paginated(
        &self,
        query_str: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<String>, usize), AppError> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.name_field, self.phone_field, self.pinyin_field, self.notes_field],
        );

        let query_with_prefix = format!("{}*", query_str.trim());
        let query = query_parser.parse_query(&query_with_prefix)
            .or_else(|_| query_parser.parse_query(query_str))
            .map_err(|e| AppError::internal(format!("解析搜索查询失败: {}", e)))?;

        let (top_docs, total) = searcher.search(
            &query,
            &(TopDocs::with_limit(limit).and_offset(offset), Count),
        )
            .map_err(|e| AppError::internal(format!("搜索执行失败: {}", e)))?;

        let mut ids = Vec::with_capacity(top_docs.len());
        for (_score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)
                .map_err(|e| AppError::internal(format!("读取索引文档失败: {}", e)))?;
            if let Some(id_value) = doc.get_first(self.id_field) {
                if let Some(id_str) = id_value.as_str() {
                    ids.push(id_str.to_string());
                }
            }
        }
        Ok((ids, total))
    }

    /// Sync version for use during startup (index rebuild).
    pub fn commit_sync(&self) -> Result<(), AppError> {
        let mut writer = self.writer.blocking_lock();
        writer.commit()
            .map_err(|e| AppError::internal(format!("索引提交失败: {}", e)))?;
        self.reader
            .reload()
            .map_err(|e| AppError::internal(format!("索引刷新失败: {}", e)))?;
        Ok(())
    }

    pub async fn commit(&self) -> Result<(), AppError> {
        let mut writer = self.writer.lock().await;
        writer.commit()
            .map_err(|e| AppError::internal(format!("索引提交失败: {}", e)))?;
        self.reader
            .reload()
            .map_err(|e| AppError::internal(format!("索引刷新失败: {}", e)))?;
        Ok(())
    }
}

/// Build pinyin text for indexing: full pinyin + initials, space-separated
fn build_pinyin_text(name: &str) -> String {
    let mut full = String::with_capacity(name.len() * 3);
    let mut initials = String::with_capacity(name.len());
    for c in name.chars() {
        match c.to_pinyin() {
            Some(py) => {
                full.push_str(py.plain());
                if let Some(first) = py.plain().chars().next() {
                    initials.push(first);
                }
            }
            None => {
                full.push(c.to_ascii_lowercase());
                initials.push(c.to_ascii_lowercase());
            }
        }
    }
    format!("{} {}", full, initials)
}
