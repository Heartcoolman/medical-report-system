//! 算法引擎集成测试 — 模拟真实医疗报告场景
//!
//! 测试场景:
//! 1. 患者一次住院上传多份报告 → 分组 + 合并检测
//! 2. 不同医院的检验项目名称 → 标准化归一
//! 3. 新报告与数据库已有报告的合并判断

#[cfg(test)]
mod tests {
    use crate::algorithm_engine::grouping_engine::*;
    use crate::algorithm_engine::name_normalizer::*;
    use crate::algorithm_engine::report_taxonomy;
    use std::collections::HashMap;

    // =========================================================================
    // 场景一：患者单次住院，一次性上传 6 份报告
    // 预期: 脑脊液(常规+生化+免疫)分一组, 乙肝(五项+DNA)分一组, 尿常规独立
    // =========================================================================
    #[test]
    fn scenario_batch_upload_grouping() {
        let items_csf_routine: Vec<String> =
            vec!["潘氏试验".into(), "白细胞计数".into(), "红细胞计数".into()];
        let items_csf_biochem: Vec<String> = vec!["葡萄糖".into(), "氯".into(), "蛋白质".into()];
        let items_csf_immune: Vec<String> = vec![
            "免疫球蛋白G".into(),
            "免疫球蛋白A".into(),
            "免疫球蛋白M".into(),
        ];
        let items_hbv_five: Vec<String> = vec![
            "乙肝表面抗原".into(),
            "乙肝表面抗体".into(),
            "乙肝e抗原".into(),
        ];
        let items_hbv_dna: Vec<String> = vec!["乙肝病毒DNA定量".into()];
        let items_urine: Vec<String> = vec!["尿蛋白".into(), "尿糖".into(), "白细胞".into()];

        let files = vec![
            ReportInfo {
                report_type: "脑脊液常规",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &items_csf_routine,
            },
            ReportInfo {
                report_type: "脑脊液生化",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &items_csf_biochem,
            },
            ReportInfo {
                report_type: "脑脊液免疫",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &items_csf_immune,
            },
            ReportInfo {
                report_type: "乙肝五项",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &items_hbv_five,
            },
            ReportInfo {
                report_type: "乙肝病毒DNA",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &items_hbv_dna,
            },
            ReportInfo {
                report_type: "尿常规",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &items_urine,
            },
        ];

        let result = batch_group(&files, &[]);

        // 脑脊液三份应在同一组
        assert!(result.groups[0] > 0, "脑脊液常规应有分组");
        assert_eq!(result.groups[0], result.groups[1], "脑脊液常规和生化应同组");
        assert_eq!(result.groups[0], result.groups[2], "脑脊液常规和免疫应同组");

        // 乙肝两份应在同一组
        assert!(result.groups[3] > 0, "乙肝五项应有分组");
        assert_eq!(result.groups[3], result.groups[4], "乙肝五项和DNA应同组");

        // 脑脊液组 ≠ 乙肝组
        assert_ne!(result.groups[0], result.groups[3], "脑脊液和乙肝不应同组");

        // 尿常规独立
        assert_eq!(result.groups[5], 0, "尿常规应独立");
    }

    // =========================================================================
    // 场景二：新报告与数据库已有报告的合并检测
    // 数据库中有: 脑脊液常规(3/15), 血常规(3/15)
    // 新上传: 脑脊液生化(3/15), 肝功能(3/15)
    // 预期: 脑脊液生化合并到脑脊液常规, 肝功能独立
    // =========================================================================
    #[test]
    fn scenario_merge_with_existing_reports() {
        let existing = vec![
            ExistingReportInfo {
                report_type: "脑脊液常规".into(),
                report_date: "2024-03-15".into(),
                sample_date: "2024-03-15".into(),
                item_names: vec!["潘氏试验".into(), "白细胞计数".into()],
            },
            ExistingReportInfo {
                report_type: "血常规".into(),
                report_date: "2024-03-15".into(),
                sample_date: "2024-03-15".into(),
                item_names: vec!["白细胞计数".into(), "红细胞计数".into(), "血红蛋白".into()],
            },
        ];

        let new_csf_items: Vec<String> = vec!["葡萄糖".into(), "氯".into(), "蛋白质".into()];
        let new_liver_items: Vec<String> =
            vec!["丙氨酸氨基转移酶".into(), "天门冬氨酸氨基转移酶".into()];

        let new_reports = vec![
            ReportInfo {
                report_type: "脑脊液生化",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &new_csf_items,
            },
            ReportInfo {
                report_type: "肝功能",
                report_date: "2024-03-15",
                sample_date: "2024-03-15",
                item_names: &new_liver_items,
            },
        ];

        let candidates = check_merge_candidates(&new_reports, &existing);

        // 脑脊液生化应与脑脊液常规(index=0)合并
        let csf_merge = candidates.iter().find(|c| c.new_report_index == 0);
        assert!(csf_merge.is_some(), "脑脊液生化应有合并候选");
        let c = csf_merge.unwrap();
        assert_eq!(c.existing_report_index, 0, "应合并到脑脊液常规");
        assert_eq!(c.score.decision, MergeDecision::Merge);

        // 肝功能不应与血常规或脑脊液合并
        let liver_merge = candidates
            .iter()
            .find(|c| c.new_report_index == 1 && c.score.decision == MergeDecision::Merge);
        assert!(liver_merge.is_none(), "肝功能不应与任何已有报告合并");
    }

    // =========================================================================
    // 场景三：不同日期的同类报告不应合并
    // =========================================================================
    #[test]
    fn scenario_different_dates_no_merge() {
        let existing = vec![ExistingReportInfo {
            report_type: "脑脊液常规".into(),
            report_date: "2024-01-10".into(),
            sample_date: "2024-01-10".into(),
            item_names: vec!["潘氏试验".into()],
        }];

        let new_items: Vec<String> = vec!["葡萄糖".into(), "氯".into()];
        let new_reports = vec![ReportInfo {
            report_type: "脑脊液生化",
            report_date: "2024-03-15",
            sample_date: "2024-03-15",
            item_names: &new_items,
        }];

        let candidates = check_merge_candidates(&new_reports, &existing);
        let merge_found = candidates
            .iter()
            .any(|c| c.score.decision == MergeDecision::Merge);
        assert!(!merge_found, "不同日期(1月vs3月)的脑脊液报告不应合并");
    }

    // =========================================================================
    // 场景四：真实 OCR 扫描出的检验项目名称标准化
    // 模拟三家不同医院对同一批检查的不同命名
    // =========================================================================
    #[test]
    fn scenario_name_normalize_multi_hospital() {
        let mut by_type = HashMap::new();

        // 医院A: 血常规
        by_type.insert(
            "血常规".to_string(),
            vec![
                "WBC".to_string(),
                "RBC".to_string(),
                "HGB".to_string(),
                "PLT".to_string(),
                "白细胞数".to_string(),         // 应 → 白细胞计数
                "红细胞总数".to_string(),       // 应 → 红细胞计数
                "中性粒细胞绝对值".to_string(), // 应 → 中性粒细胞计数
                "血细胞比容".to_string(),       // 旧称 → 红细胞压积
            ],
        );

        // 医院B: 肝功能
        by_type.insert(
            "肝功能".to_string(),
            vec![
                "谷丙转氨酶".to_string(),     // 旧称 → 丙氨酸氨基转移酶
                "谷草转氨酶".to_string(),     // 旧称 → 天门冬氨酸氨基转移酶
                "白蛋白（比色）".to_string(), // 方法后缀 → 白蛋白
                "ALT".to_string(),            // 英文缩写 → 丙氨酸氨基转移酶
                "AST".to_string(),            // 英文缩写 → 天门冬氨酸氨基转移酶
            ],
        );

        // 医院C: 乙肝相关
        by_type.insert(
            "乙肝检查".to_string(),
            vec![
                "HBV-DNA".to_string(),
                "高敏HBV-DNA定量".to_string(),
                "超高敏乙型肝炎病毒DNA".to_string(),
                "乙肝表面抗原定量".to_string(),
                "HBsAg".to_string(),
            ],
        );

        let results = normalize_batch(&by_type, &[]);
        let (resolved, unresolved) = split_results(&results);

        // --- 血常规项目 ---
        assert_eq!(resolved.get("WBC").unwrap(), "白细胞计数", "WBC应标准化");
        assert_eq!(resolved.get("RBC").unwrap(), "红细胞计数", "RBC应标准化");
        assert_eq!(resolved.get("HGB").unwrap(), "血红蛋白", "HGB应标准化");
        assert_eq!(resolved.get("PLT").unwrap(), "血小板计数", "PLT应标准化");
        assert_eq!(
            resolved.get("白细胞数").unwrap(),
            "白细胞计数",
            "白细胞数→白细胞计数"
        );
        assert_eq!(
            resolved.get("红细胞总数").unwrap(),
            "红细胞计数",
            "红细胞总数→红细胞计数"
        );
        assert_eq!(resolved.get("中性粒细胞绝对值").unwrap(), "中性粒细胞计数");
        assert_eq!(
            resolved.get("血细胞比容").unwrap(),
            "红细胞压积",
            "旧称→标准名"
        );

        // --- 肝功能项目 ---
        assert_eq!(resolved.get("谷丙转氨酶").unwrap(), "丙氨酸氨基转移酶");
        assert_eq!(resolved.get("谷草转氨酶").unwrap(), "天门冬氨酸氨基转移酶");
        assert_eq!(
            resolved.get("白蛋白（比色）").unwrap(),
            "白蛋白",
            "去除方法后缀"
        );
        assert_eq!(
            resolved.get("ALT").unwrap(),
            "丙氨酸氨基转移酶",
            "ALT→中文标准名"
        );
        assert_eq!(
            resolved.get("AST").unwrap(),
            "天门冬氨酸氨基转移酶",
            "AST→中文标准名"
        );
        // 谷丙转氨酶 和 ALT 应标准化为同一个名称
        assert_eq!(
            resolved.get("谷丙转氨酶").unwrap(),
            resolved.get("ALT").unwrap(),
            "谷丙转氨酶和ALT应为同一标准名"
        );

        // --- 乙肝项目 ---
        assert_eq!(resolved.get("HBV-DNA").unwrap(), "乙肝病毒DNA定量");
        assert_eq!(resolved.get("高敏HBV-DNA定量").unwrap(), "乙肝病毒DNA定量");
        assert_eq!(
            resolved.get("超高敏乙型肝炎病毒DNA").unwrap(),
            "乙肝病毒DNA定量"
        );
        assert_eq!(resolved.get("HBsAg").unwrap(), "乙肝表面抗原");
        assert_eq!(
            resolved.get("乙肝表面抗原定量").unwrap(),
            "乙肝表面抗原",
            "去除定量后缀"
        );
        // 所有 HBV-DNA 变体应归一
        assert_eq!(
            resolved.get("HBV-DNA").unwrap(),
            resolved.get("高敏HBV-DNA定量").unwrap(),
            "所有HBV-DNA变体应归一"
        );
        assert_eq!(
            resolved.get("HBV-DNA").unwrap(),
            resolved.get("超高敏乙型肝炎病毒DNA").unwrap(),
            "中英文HBV-DNA应归一"
        );

        // 打印未解决的名称（如果有）
        if !unresolved.is_empty() {
            println!("未解决名称 (需 LLM): {:?}", unresolved);
        }
        // 此场景中所有名称都应该被算法引擎解决
        assert!(
            unresolved.is_empty(),
            "此场景应全部由算法引擎解决，但有 {} 个未解决: {:?}",
            unresolved.len(),
            unresolved
        );
    }

    // =========================================================================
    // 场景五：脑脊液报告中的体液前缀去重
    // 同一报告内「免疫球蛋白A」和「脑脊液免疫球蛋白A」是同一指标
    // =========================================================================
    #[test]
    fn scenario_csf_fluid_prefix_dedup() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "脑脊液生化".to_string(),
            vec![
                "脑脊液葡萄糖".to_string(),
                "葡萄糖".to_string(),
                "脑脊液氯".to_string(),
                "氯".to_string(),
                "脑脊液蛋白质".to_string(),
                "蛋白质".to_string(),
            ],
        );

        let results = normalize_batch(&by_type, &[]);

        // 「脑脊液葡萄糖」在脑脊液报告中应去掉前缀 → 「葡萄糖」
        let csf_glucose = &results["脑脊液葡萄糖"];
        let plain_glucose = &results["葡萄糖"];
        assert_eq!(
            csf_glucose.canonical, plain_glucose.canonical,
            "脑脊液葡萄糖 和 葡萄糖 在脑脊液报告中应归一: got '{}' vs '{}'",
            csf_glucose.canonical, plain_glucose.canonical
        );

        assert_eq!(csf_glucose.canonical, plain_glucose.canonical);
        assert_eq!(
            results["脑脊液氯"].canonical, results["氯"].canonical,
            "脑脊液氯 和 氯 应归一"
        );
    }

    // =========================================================================
    // 场景六：fuzzy match — 新名称与系统已有标准名模糊匹配
    // =========================================================================
    #[test]
    fn scenario_fuzzy_match_existing_canonical() {
        let existing_canonical = vec![
            "丙氨酸氨基转移酶".to_string(),
            "天门冬氨酸氨基转移酶".to_string(),
            "白细胞计数".to_string(),
            "红细胞计数".to_string(),
            "血红蛋白".to_string(),
            "中性粒细胞计数".to_string(),
        ];

        let mut by_type = HashMap::new();
        // 模拟 OCR 轻微识别错误或不同表达
        by_type.insert(
            "血常规".to_string(),
            vec![
                "中性粒细胞记数".to_string(), // 「记数」vs「计数」→ 应 fuzzy match
            ],
        );

        let results = normalize_batch(&by_type, &existing_canonical);
        let r = &results["中性粒细胞记数"];

        // 「记数」已被规则层归一为「计数」，然后字典匹配
        assert_eq!(
            r.canonical, "中性粒细胞计数",
            "「记数」应归一为「计数」: got '{}'",
            r.canonical
        );
        assert!(
            r.method == NormalizeMethod::Dictionary || r.method == NormalizeMethod::FuzzyMatch,
            "应通过规则+字典或模糊匹配解决, got {:?}",
            r.method
        );
    }

    // =========================================================================
    // 场景七：报告类型分类 — 真实临床报告类型判断
    // =========================================================================
    #[test]
    fn scenario_taxonomy_clinical_types() {
        // 同类检查应识别
        let pairs_same = vec![
            ("脑脊液常规", "脑脊液生化"),
            ("脑脊液常规", "脑脊液免疫球蛋白"),
            ("乙肝五项", "乙肝病毒DNA"),
            ("凝血功能", "凝血四项"),
            ("血常规", "血常规五分类"),
            ("肝功能", "肝功能全套"),
        ];
        for (a, b) in &pairs_same {
            let m = report_taxonomy::same_category(a, b);
            assert!(
                m.same_category,
                "{} 和 {} 应为同类, confidence={}",
                a, b, m.confidence
            );
        }

        // 不同类检查应区分
        let pairs_diff = vec![
            ("血常规", "尿常规"),
            ("肝功能", "脑脊液常规"),
            ("血常规", "乙肝五项"),
            ("尿常规", "凝血功能"),
        ];
        for (a, b) in &pairs_diff {
            let m = report_taxonomy::same_category(a, b);
            assert!(!m.same_category, "{} 和 {} 不应为同类", a, b);
        }
    }

    // =========================================================================
    // 场景八：端到端 — 完整模拟从上传到标准化的流程
    // 患者上传3份报告, 数据库已有1份, 算法做分组+合并+标准化
    // =========================================================================
    #[test]
    fn scenario_end_to_end() {
        // --- Step 1: 数据库已有报告 ---
        let existing_reports = vec![ExistingReportInfo {
            report_type: "脑脊液常规".into(),
            report_date: "2024-06-20".into(),
            sample_date: "2024-06-20".into(),
            item_names: vec!["潘氏试验".into(), "白细胞计数".into(), "红细胞计数".into()],
        }];

        // --- Step 2: 新上传 3 份报告 ---
        let items_new_csf: Vec<String> = vec!["葡萄糖".into(), "氯".into(), "蛋白质".into()];
        let items_new_blood: Vec<String> = vec!["WBC".into(), "RBC".into(), "HGB".into()];
        let items_new_liver: Vec<String> = vec!["谷丙转氨酶".into(), "谷草转氨酶".into()];

        let new_files = vec![
            ReportInfo {
                report_type: "脑脊液生化",
                report_date: "2024-06-20",
                sample_date: "2024-06-20",
                item_names: &items_new_csf,
            },
            ReportInfo {
                report_type: "血常规",
                report_date: "2024-06-20",
                sample_date: "2024-06-20",
                item_names: &items_new_blood,
            },
            ReportInfo {
                report_type: "肝功能",
                report_date: "2024-06-20",
                sample_date: "2024-06-20",
                item_names: &items_new_liver,
            },
        ];

        // --- Step 3: 分组 ---
        let group_result = batch_group(&new_files, &existing_reports);

        // 脑脊液生化应与已有脑脊液常规合并
        assert!(
            !group_result.existing_merges.is_empty(),
            "应有合并到已有报告"
        );
        let csf_merge = group_result.existing_merges.iter().find(|(ni, _)| *ni == 0);
        assert!(csf_merge.is_some(), "脑脊液生化应合并到已有报告");
        assert_eq!(csf_merge.unwrap().1, 0, "应合并到第0个已有报告(脑脊液常规)");

        // 血常规和肝功能各自独立
        assert_eq!(group_result.groups[1], 0, "血常规应独立");
        assert_eq!(group_result.groups[2], 0, "肝功能应独立");

        // --- Step 4: 名称标准化 ---
        let mut all_items_by_type = HashMap::new();
        all_items_by_type.insert("脑脊液生化".to_string(), items_new_csf.clone());
        all_items_by_type.insert("血常规".to_string(), items_new_blood.clone());
        all_items_by_type.insert("肝功能".to_string(), items_new_liver.clone());

        let existing_canonical = vec![
            "潘氏试验".to_string(),
            "白细胞计数".to_string(),
            "红细胞计数".to_string(),
        ];

        let norm_results = normalize_batch(&all_items_by_type, &existing_canonical);
        let (resolved, unresolved) = split_results(&norm_results);

        // 血常规缩写应被解析
        assert_eq!(resolved["WBC"], "白细胞计数");
        assert_eq!(resolved["RBC"], "红细胞计数");
        assert_eq!(resolved["HGB"], "血红蛋白");

        // 肝功能旧称应标准化
        assert_eq!(resolved["谷丙转氨酶"], "丙氨酸氨基转移酶");
        assert_eq!(resolved["谷草转氨酶"], "天门冬氨酸氨基转移酶");

        // 未解决的应为空或很少
        println!(
            "端到端测试 - 已解决: {}, 未解决: {} {:?}",
            resolved.len(),
            unresolved.len(),
            unresolved
        );

        assert!(
            unresolved.len() <= 3,
            "未解决名称不应超过3个 (脑脊液生化项目可能未在字典中)"
        );
    }

    // =========================================================================
    // 场景九：相邻日期(±1天)的同类报告应合并
    // 真实情况：采样日期是3/15，报告日期可能是3/16
    // =========================================================================
    #[test]
    fn scenario_adjacent_date_merge() {
        let items_a: Vec<String> = vec!["潘氏试验".into(), "白细胞计数".into()];
        let items_b: Vec<String> = vec!["葡萄糖".into(), "氯".into()];

        let a = ReportInfo {
            report_type: "脑脊液常规",
            report_date: "2024-03-15",
            sample_date: "",
            item_names: &items_a,
        };
        let b = ReportInfo {
            report_type: "脑脊液生化",
            report_date: "2024-03-16",
            sample_date: "",
            item_names: &items_b,
        };

        let score = compute_merge_score(&a, &b);
        assert_eq!(
            score.decision,
            MergeDecision::Merge,
            "相邻日期的同类脑脊液报告应合并, total={:.2}",
            score.total
        );
    }

    // =========================================================================
    // 场景十一（V3）：不同类别 + 同日 + 互补项目 → 绝对不合并
    // 此前 bug: 血常规+肝功能同日互补 → Uncertain(0.6)，应为 NoMerge
    // =========================================================================
    #[test]
    fn v3_different_category_same_day_no_merge() {
        let items_blood: Vec<String> = vec![
            "白细胞计数".into(),
            "红细胞计数".into(),
            "血红蛋白".into(),
            "血小板计数".into(),
        ];
        let items_liver: Vec<String> = vec![
            "丙氨酸氨基转移酶".into(),
            "天门冬氨酸氨基转移酶".into(),
            "总胆红素".into(),
            "白蛋白".into(),
        ];

        let a = ReportInfo {
            report_type: "血常规",
            report_date: "2024-03-15",
            sample_date: "2024-03-15",
            item_names: &items_blood,
        };
        let b = ReportInfo {
            report_type: "肝功能",
            report_date: "2024-03-15",
            sample_date: "2024-03-15",
            item_names: &items_liver,
        };

        let score = compute_merge_score(&a, &b);
        assert_eq!(
            score.decision,
            MergeDecision::NoMerge,
            "血常规和肝功能即使同日互补也不应合并"
        );
    }

    // =========================================================================
    // 场景十二（V3）：同类型 + 同日 + 完全相同项目 → 复查检测，不合并
    // =========================================================================
    #[test]
    fn v3_recheck_detection() {
        let items: Vec<String> = vec![
            "白细胞计数".into(),
            "红细胞计数".into(),
            "血红蛋白".into(),
            "血小板计数".into(),
            "中性粒细胞计数".into(),
        ];

        let a = ReportInfo {
            report_type: "血常规",
            report_date: "2024-03-15",
            sample_date: "2024-03-15",
            item_names: &items,
        };
        let b = ReportInfo {
            report_type: "血常规",
            report_date: "2024-03-15",
            sample_date: "2024-03-15",
            item_names: &items,
        };

        let score = compute_merge_score(&a, &b);
        assert_eq!(
            score.decision,
            MergeDecision::NoMerge,
            "同日同类同项目是复查，不应合并"
        );
    }

    // =========================================================================
    // 场景十三（V3）：项目签名交叉验证 — OCR 类型正确时确认合并
    // =========================================================================
    #[test]
    fn v3_profile_confirmed_merge() {
        // 脑脊液常规+生化, 项目都是脑脊液相关
        let items_routine: Vec<String> = vec![
            "潘氏试验".into(),
            "白细胞计数".into(),
            "红细胞计数".into(),
        ];
        let items_biochem: Vec<String> = vec!["葡萄糖".into(), "氯".into(), "蛋白质".into()];

        let a = ReportInfo {
            report_type: "脑脊液常规",
            report_date: "2024-03-15",
            sample_date: "2024-03-15",
            item_names: &items_routine,
        };
        let b = ReportInfo {
            report_type: "脑脊液生化",
            report_date: "2024-03-15",
            sample_date: "2024-03-15",
            item_names: &items_biochem,
        };

        let score = compute_merge_score(&a, &b);
        assert_eq!(
            score.decision,
            MergeDecision::Merge,
            "脑脊液常规+生化同日互补应合并"
        );
    }

    // =========================================================================
    // 场景十四（V3）：±2-3天 → 无论类型如何都不合并
    // =========================================================================
    #[test]
    fn v3_close_date_always_no_merge() {
        let items_a: Vec<String> = vec!["潘氏试验".into(), "白细胞计数".into()];
        let items_b: Vec<String> = vec!["葡萄糖".into(), "氯".into()];

        // 2 天差
        let a = ReportInfo {
            report_type: "脑脊液常规",
            report_date: "2024-03-15",
            sample_date: "",
            item_names: &items_a,
        };
        let b = ReportInfo {
            report_type: "脑脊液生化",
            report_date: "2024-03-17",
            sample_date: "",
            item_names: &items_b,
        };
        let score = compute_merge_score(&a, &b);
        assert_eq!(
            score.decision,
            MergeDecision::NoMerge,
            "2天差的脑脊液报告不应合并"
        );

        // 3 天差
        let c = ReportInfo {
            report_type: "脑脊液生化",
            report_date: "2024-03-18",
            sample_date: "",
            item_names: &items_b,
        };
        let score2 = compute_merge_score(&a, &c);
        assert_eq!(
            score2.decision,
            MergeDecision::NoMerge,
            "3天差的脑脊液报告不应合并"
        );
    }

    // =========================================================================
    // 场景十五（V3）：未知类型 → Uncertain（需LLM验证）
    // =========================================================================
    #[test]
    fn v3_unknown_type_uncertain() {
        let items_a: Vec<String> = vec!["某指标A".into(), "某指标B".into()];
        let items_b: Vec<String> = vec!["某指标C".into(), "某指标D".into()];

        let a = ReportInfo {
            report_type: "特殊检查XYZ",
            report_date: "2024-03-15",
            sample_date: "",
            item_names: &items_a,
        };
        let b = ReportInfo {
            report_type: "特殊检查XYZ分析",
            report_date: "2024-03-15",
            sample_date: "",
            item_names: &items_b,
        };
        let score = compute_merge_score(&a, &b);
        assert_eq!(
            score.decision,
            MergeDecision::Uncertain,
            "未知类型应为 Uncertain 交给 LLM 验证"
        );
    }

    // =========================================================================
    // 场景十六（V3）：Unicode 归一化 — 全角字符标准化
    // =========================================================================
    #[test]
    fn v3_unicode_normalize() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "肝功能".to_string(),
            vec![
                "\u{FF21}\u{FF2C}\u{FF34}".to_string(), // ＡＬＴ (fullwidth)
                "γ\u{FF0D}谷氨酰转移酶".to_string(),    // fullwidth dash
            ],
        );

        let results = normalize_batch(&by_type, &[]);
        let (resolved, _) = split_results(&results);

        assert_eq!(
            resolved.get("\u{FF21}\u{FF2C}\u{FF34}").unwrap(),
            "丙氨酸氨基转移酶",
            "全角ALT应标准化"
        );
    }

    // =========================================================================
    // 场景十七（V3）：方法后缀扩展剥离
    // =========================================================================
    #[test]
    fn v3_method_suffix_stripping() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "肝功能".to_string(),
            vec![
                "丙氨酸氨基转移酶测定".to_string(),
                "天门冬氨酸氨基转移酶检测".to_string(),
                "碱性磷酸酶检查".to_string(),
            ],
        );

        let results = normalize_batch(&by_type, &[]);
        let (resolved, _) = split_results(&results);

        assert_eq!(
            resolved.get("丙氨酸氨基转移酶测定").unwrap(),
            "丙氨酸氨基转移酶",
            "测定后缀应剥离"
        );
        assert_eq!(
            resolved.get("天门冬氨酸氨基转移酶检测").unwrap(),
            "天门冬氨酸氨基转移酶",
            "检测后缀应剥离"
        );
        assert_eq!(
            resolved.get("碱性磷酸酶检查").unwrap(),
            "碱性磷酸酶",
            "检查后缀应剥离"
        );
    }

    // =========================================================================
    // 场景十：压力测试 — 大量项目名称的标准化性能
    // =========================================================================
    #[test]
    fn scenario_bulk_normalization_performance() {
        let mut by_type = HashMap::new();
        let mut blood_items = Vec::new();
        let base_names = vec![
            "WBC",
            "RBC",
            "HGB",
            "PLT",
            "白细胞数",
            "红细胞总数",
            "中性粒细胞绝对值",
            "淋巴细胞绝对值",
            "单核细胞绝对值",
            "血红蛋白",
            "红细胞压积",
            "平均红细胞体积",
        ];
        // 重复生成 100 个（模拟多份报告的项目汇总）
        for i in 0..100 {
            blood_items.push(base_names[i % base_names.len()].to_string());
        }
        by_type.insert("血常规".to_string(), blood_items);

        let t0 = std::time::Instant::now();
        let results = normalize_batch(&by_type, &[]);
        let elapsed = t0.elapsed();

        assert!(!results.is_empty());
        // 应在 10ms 内完成（实际远小于此）
        assert!(
            elapsed.as_millis() < 10,
            "100 个名称标准化应在 10ms 内完成, 实际: {}ms",
            elapsed.as_millis()
        );

        println!(
            "批量标准化性能: {} 个名称, 耗时 {:?}, 结果 {} 条",
            100,
            elapsed,
            results.len()
        );
    }
}
