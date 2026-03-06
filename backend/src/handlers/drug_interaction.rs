use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{run_blocking, AppError};
use crate::models::ApiResponse;
use crate::AppState;

// ---------------------------------------------------------------------------
// Static interaction knowledge base
// ---------------------------------------------------------------------------

struct InteractionRule {
    drug1_keywords: &'static [&'static str],
    drug2_keywords: &'static [&'static str],
    severity: &'static str,
    description: &'static str,
    recommendation: &'static str,
}

static INTERACTION_RULES: &[InteractionRule] = &[
    // 1. 华法林 + NSAIDs
    InteractionRule {
        drug1_keywords: &["华法林"],
        drug2_keywords: &["布洛芬", "阿司匹林", "吲哚美辛", "双氯芬酸", "萘普生", "美洛昔康", "塞来昔布"],
        severity: "high",
        description: "华法林与非甾体抗炎药(NSAIDs)合用，显著增加出血风险，可能导致消化道出血或其他部位出血",
        recommendation: "避免合用；如必须使用止痛药，优先选择对乙酰氨基酚（扑热息痛）；密切监测INR和出血征象",
    },
    // 2. 华法林 + 头孢类抗生素
    InteractionRule {
        drug1_keywords: &["华法林"],
        drug2_keywords: &["头孢", "头孢曲松", "头孢哌酮", "头孢唑林", "头孢呋辛"],
        severity: "high",
        description: "部分头孢类抗生素可增强华法林抗凝作用，增加出血风险",
        recommendation: "合用期间加强INR监测，必要时调整华法林剂量",
    },
    // 3. 华法林 + 甲硝唑
    InteractionRule {
        drug1_keywords: &["华法林"],
        drug2_keywords: &["甲硝唑"],
        severity: "high",
        description: "甲硝唑抑制华法林代谢，显著增强抗凝效果，增加出血风险",
        recommendation: "尽量避免合用；如必须合用，密切监测INR，减少华法林剂量",
    },
    // 4. 他汀类 + 贝特类
    InteractionRule {
        drug1_keywords: &["阿托伐他汀", "辛伐他汀", "瑞舒伐他汀", "洛伐他汀", "普伐他汀", "氟伐他汀", "他汀"],
        drug2_keywords: &["非诺贝特", "吉非贝齐", "苯扎贝特", "贝特"],
        severity: "high",
        description: "他汀类与贝特类合用可增加横纹肌溶解风险，尤其是吉非贝齐与他汀合用风险最高",
        recommendation: "尽量避免合用吉非贝齐与他汀；如需联合降脂，优先选择非诺贝特；监测肌酸激酶(CK)和肌肉症状",
    },
    // 5. ACE抑制剂 + 保钾利尿剂
    InteractionRule {
        drug1_keywords: &["卡托普利", "依那普利", "雷米普利", "培哚普利", "贝那普利", "福辛普利", "赖诺普利", "普利"],
        drug2_keywords: &["螺内酯", "氨苯蝶啶", "阿米洛利"],
        severity: "high",
        description: "ACE抑制剂与保钾利尿剂合用可导致高钾血症，严重时可引起心律失常",
        recommendation: "合用时密切监测血钾水平；避免同时补钾；定期检查肾功能和电解质",
    },
    // 6. ACE抑制剂 + ARB
    InteractionRule {
        drug1_keywords: &["卡托普利", "依那普利", "雷米普利", "培哚普利", "贝那普利", "福辛普利", "赖诺普利", "普利"],
        drug2_keywords: &["厄贝沙坦", "缬沙坦", "氯沙坦", "替米沙坦", "坎地沙坦", "奥美沙坦", "沙坦"],
        severity: "high",
        description: "ACE抑制剂与ARB双重阻断RAS系统，增加低血压、高钾血症和肾损害风险",
        recommendation: "一般不建议合用；如有特殊指征需合用，密切监测血压、血钾和肾功能",
    },
    // 7. 喹诺酮 + 抗酸药
    InteractionRule {
        drug1_keywords: &["左氧氟沙星", "环丙沙星", "莫西沙星", "诺氟沙星", "氧氟沙星", "喹诺酮", "沙星"],
        drug2_keywords: &["碳酸钙", "氢氧化铝", "铝碳酸镁", "氧化镁"],
        severity: "medium",
        description: "含金属离子的抗酸药可与喹诺酮类抗生素螯合，显著降低抗生素吸收和疗效",
        recommendation: "两药服用间隔至少2小时以上；先服喹诺酮类，后服抗酸药",
    },
    // 8. 地高辛 + 胺碘酮
    InteractionRule {
        drug1_keywords: &["地高辛"],
        drug2_keywords: &["胺碘酮"],
        severity: "high",
        description: "胺碘酮可使地高辛血药浓度升高70-100%，显著增加地高辛中毒风险",
        recommendation: "合用时地高辛剂量应减半；密切监测地高辛血药浓度和心电图",
    },
    // 9. 地高辛 + 利尿剂
    InteractionRule {
        drug1_keywords: &["地高辛"],
        drug2_keywords: &["氢氯噻嗪", "呋塞米", "吲达帕胺", "托拉塞米", "噻嗪"],
        severity: "medium",
        description: "排钾利尿剂导致的低钾血症可增加地高辛毒性风险，出现心律失常",
        recommendation: "监测血钾水平，必要时补钾或联用保钾利尿剂；监测地高辛血药浓度",
    },
    // 10. 氨茶碱 + 喹诺酮类
    InteractionRule {
        drug1_keywords: &["氨茶碱", "茶碱"],
        drug2_keywords: &["环丙沙星", "依诺沙星", "左氧氟沙星", "诺氟沙星", "喹诺酮"],
        severity: "high",
        description: "部分喹诺酮类抑制茶碱代谢，可导致茶碱血药浓度升高，引起茶碱中毒（恶心、呕吐、心悸、抽搐）",
        recommendation: "避免合用环丙沙星/依诺沙星与茶碱；如需合用，监测茶碱血药浓度，调整剂量",
    },
    // 11. SSRI + 曲马多
    InteractionRule {
        drug1_keywords: &["氟西汀", "帕罗西汀", "舍曲林", "西酞普兰", "艾司西酞普兰", "氟伏沙明"],
        drug2_keywords: &["曲马多"],
        severity: "high",
        description: "SSRI与曲马多合用可导致5-羟色胺综合征，表现为高热、肌阵挛、精神状态改变",
        recommendation: "尽量避免合用；如需镇痛考虑其他药物；出现发热、肌肉僵直等症状立即就医",
    },
    // 12. 甲氨蝶呤 + NSAIDs
    InteractionRule {
        drug1_keywords: &["甲氨蝶呤"],
        drug2_keywords: &["布洛芬", "阿司匹林", "吲哚美辛", "双氯芬酸", "萘普生"],
        severity: "high",
        description: "NSAIDs可减少甲氨蝶呤肾排泄，显著增加甲氨蝶呤毒性（骨髓抑制、肝毒性）",
        recommendation: "高剂量甲氨蝶呤禁止与NSAIDs合用；低剂量需谨慎，监测血常规和肝肾功能",
    },
    // 13. 氯吡格雷 + 奥美拉唑
    InteractionRule {
        drug1_keywords: &["氯吡格雷"],
        drug2_keywords: &["奥美拉唑", "埃索美拉唑"],
        severity: "medium",
        description: "奥美拉唑抑制CYP2C19，可降低氯吡格雷活性代谢物生成，减弱抗血小板作用",
        recommendation: "优先选择泮托拉唑或雷贝拉唑替代；避免使用奥美拉唑和埃索美拉唑",
    },
    // 14. 二甲双胍 + 碘对比剂
    InteractionRule {
        drug1_keywords: &["二甲双胍"],
        drug2_keywords: &["碘对比剂", "造影剂", "碘海醇", "碘佛醇"],
        severity: "high",
        description: "碘对比剂可引起急性肾损伤，与二甲双胍合用增加乳酸酸中毒风险",
        recommendation: "检查前48小时停用二甲双胍；检查后确认肾功能正常再恢复用药",
    },
    // 15. 胰岛素 + beta受体阻滞剂
    InteractionRule {
        drug1_keywords: &["胰岛素"],
        drug2_keywords: &["普萘洛尔", "美托洛尔", "阿替洛尔", "比索洛尔", "卡维地洛"],
        severity: "medium",
        description: "beta受体阻滞剂可掩盖低血糖的心悸、震颤等预警症状，延误低血糖识别",
        recommendation: "加强血糖监测；教育患者识别非肾上腺素能低血糖症状（出汗、饥饿感）；优选选择性beta1阻滞剂",
    },
    // 16. 锂盐 + NSAIDs
    InteractionRule {
        drug1_keywords: &["碳酸锂", "锂盐"],
        drug2_keywords: &["布洛芬", "吲哚美辛", "双氯芬酸", "萘普生", "美洛昔康"],
        severity: "high",
        description: "NSAIDs减少锂的肾排泄，可导致锂中毒（震颤、共济失调、意识障碍）",
        recommendation: "尽量避免合用；如必须使用，监测锂血药浓度；优选对乙酰氨基酚或阿司匹林低剂量",
    },
    // 17. 卡马西平 + 口服避孕药
    InteractionRule {
        drug1_keywords: &["卡马西平"],
        drug2_keywords: &["口服避孕药", "炔雌醇", "左炔诺孕酮", "屈螺酮", "避孕药"],
        severity: "medium",
        description: "卡马西平是强效CYP3A4诱导剂，可加速避孕药代谢，导致避孕失败",
        recommendation: "建议采用非激素避孕方法或使用高剂量避孕药；告知患者避孕失效风险",
    },
    // 18. 利福平 + 华法林
    InteractionRule {
        drug1_keywords: &["利福平"],
        drug2_keywords: &["华法林"],
        severity: "high",
        description: "利福平是强效肝酶诱导剂，可显著加速华法林代谢，降低抗凝效果",
        recommendation: "合用期间需大幅增加华法林剂量；停用利福平后需减量；密切监测INR",
    },
    // 19. 利福平 + 口服避孕药
    InteractionRule {
        drug1_keywords: &["利福平"],
        drug2_keywords: &["口服避孕药", "炔雌醇", "避孕药"],
        severity: "high",
        description: "利福平显著降低口服避孕药血药浓度，导致避孕失败",
        recommendation: "使用利福平期间及停药后至少1个月内采用非激素避孕方法",
    },
    // 20. 利福平 + 他汀类
    InteractionRule {
        drug1_keywords: &["利福平"],
        drug2_keywords: &["阿托伐他汀", "辛伐他汀", "瑞舒伐他汀", "他汀"],
        severity: "medium",
        description: "利福平诱导CYP3A4，加速他汀类药物代谢，降低降脂疗效",
        recommendation: "合用时可能需要增加他汀剂量；监测血脂水平；考虑选择受CYP3A4影响较小的他汀",
    },
    // 21. SSRI + MAO抑制剂
    InteractionRule {
        drug1_keywords: &["氟西汀", "帕罗西汀", "舍曲林", "西酞普兰", "艾司西酞普兰"],
        drug2_keywords: &["司来吉兰", "吗氯贝胺", "苯乙肼", "异烟肼"],
        severity: "high",
        description: "SSRI与MAO抑制剂合用可导致致命性5-羟色胺综合征",
        recommendation: "禁止合用；停用SSRI后至少等2周（氟西汀需等5周）再使用MAO抑制剂",
    },
    // 22. 甲硝唑 + 酒精
    InteractionRule {
        drug1_keywords: &["甲硝唑"],
        drug2_keywords: &["酒精", "乙醇", "含酒精"],
        severity: "medium",
        description: "甲硝唑抑制乙醛脱氢酶，饮酒后可出现双硫仑样反应（面部潮红、恶心、呕吐、心悸）",
        recommendation: "用药期间及停药后72小时内禁酒；避免含酒精食物和药物",
    },
    // 23. 克拉霉素 + 他汀类
    InteractionRule {
        drug1_keywords: &["克拉霉素", "红霉素"],
        drug2_keywords: &["辛伐他汀", "洛伐他汀", "阿托伐他汀", "他汀"],
        severity: "high",
        description: "大环内酯类抑制CYP3A4，升高他汀血药浓度，增加横纹肌溶解风险",
        recommendation: "克拉霉素/红霉素禁止与辛伐他汀/洛伐他汀合用；可选择阿奇霉素替代",
    },
    // 24. 钙通道阻滞剂 + beta受体阻滞剂
    InteractionRule {
        drug1_keywords: &["维拉帕米", "地尔硫卓"],
        drug2_keywords: &["普萘洛尔", "美托洛尔", "阿替洛尔", "比索洛尔"],
        severity: "high",
        description: "维拉帕米/地尔硫卓与beta阻滞剂合用可导致严重心动过缓、房室传导阻滞甚至心脏停搏",
        recommendation: "避免静脉联合使用；口服合用需谨慎，监测心率和心电图",
    },
    // 25. 地高辛 + 维拉帕米
    InteractionRule {
        drug1_keywords: &["地高辛"],
        drug2_keywords: &["维拉帕米"],
        severity: "high",
        description: "维拉帕米升高地高辛血药浓度50-75%，且两药均抑制房室传导",
        recommendation: "合用时地高辛减量1/3至1/2；监测地高辛血药浓度和心电图",
    },
    // 26. 钾盐 + ACE抑制剂
    InteractionRule {
        drug1_keywords: &["氯化钾", "补钾", "钾盐"],
        drug2_keywords: &["卡托普利", "依那普利", "雷米普利", "培哚普利", "普利"],
        severity: "medium",
        description: "ACE抑制剂减少醛固酮分泌保钾，合用钾盐可导致高钾血症",
        recommendation: "监测血钾水平；避免同时补钾，除非有明确低钾",
    },
    // 27. 西地那非 + 硝酸酯类
    InteractionRule {
        drug1_keywords: &["西地那非", "他达拉非", "伐地那非"],
        drug2_keywords: &["硝酸甘油", "硝酸异山梨酯", "单硝酸异山梨酯", "硝酸酯"],
        severity: "high",
        description: "PDE5抑制剂与硝酸酯类合用可导致严重低血压甚至休克",
        recommendation: "禁止合用；使用PDE5抑制剂后24-48小时内不得使用硝酸酯类",
    },
    // 28. 丙戊酸 + 卡巴西平
    InteractionRule {
        drug1_keywords: &["丙戊酸", "丙戊酸钠", "德巴金"],
        drug2_keywords: &["卡马西平"],
        severity: "medium",
        description: "卡马西平诱导丙戊酸代谢降低其血药浓度；丙戊酸可升高卡马西平环氧化物水平增加毒性",
        recommendation: "合用需监测两药血药浓度；注意卡马西平毒性症状（头晕、复视）",
    },
    // 29. 氟康唑 + 华法林
    InteractionRule {
        drug1_keywords: &["氟康唑", "伊曲康唑", "伏立康唑"],
        drug2_keywords: &["华法林"],
        severity: "high",
        description: "唑类抗真菌药抑制CYP2C9/3A4，显著增强华法林抗凝作用",
        recommendation: "合用时减少华法林剂量；密切监测INR（开始合用后3-5天）",
    },
    // 30. 氨基糖苷类 + 呋塞米
    InteractionRule {
        drug1_keywords: &["庆大霉素", "阿米卡星", "妥布霉素", "链霉素", "氨基糖苷"],
        drug2_keywords: &["呋塞米", "依他尼酸", "布美他尼"],
        severity: "high",
        description: "袢利尿剂与氨基糖苷类合用增加耳毒性和肾毒性风险",
        recommendation: "尽量避免合用；如必须合用，监测听力和肾功能；保持充分水化",
    },
    // 31. 苯二氮卓类 + 阿片类
    InteractionRule {
        drug1_keywords: &["地西泮", "阿普唑仑", "氯硝西泮", "咪达唑仑", "劳拉西泮"],
        drug2_keywords: &["吗啡", "芬太尼", "哌替啶", "羟考酮", "曲马多", "可待因"],
        severity: "high",
        description: "苯二氮卓类与阿片类合用显著增加呼吸抑制和过度镇静风险",
        recommendation: "尽量避免合用；如必须合用使用最低有效剂量和最短疗程；监测呼吸和意识",
    },
    // 32. 二甲双胍 + 酒精
    InteractionRule {
        drug1_keywords: &["二甲双胍"],
        drug2_keywords: &["酒精", "乙醇"],
        severity: "medium",
        description: "酒精可增加二甲双胍相关乳酸酸中毒风险，并加重低血糖",
        recommendation: "限制饮酒；避免大量饮酒；出现恶心、呕吐、腹痛等症状及时就医",
    },
];

// ---------------------------------------------------------------------------
// Matching logic
// ---------------------------------------------------------------------------

fn drug_matches(drug_name: &str, keywords: &[&str]) -> bool {
    let name_lower = drug_name.to_lowercase();
    keywords.iter().any(|kw| name_lower.contains(&kw.to_lowercase()))
}

fn check_interactions(drugs: &[String]) -> Vec<DrugInteraction> {
    let mut interactions = Vec::new();
    for i in 0..drugs.len() {
        for j in (i + 1)..drugs.len() {
            for rule in INTERACTION_RULES {
                let match_a = (drug_matches(&drugs[i], rule.drug1_keywords)
                    && drug_matches(&drugs[j], rule.drug2_keywords))
                    || (drug_matches(&drugs[i], rule.drug2_keywords)
                        && drug_matches(&drugs[j], rule.drug1_keywords));
                if match_a {
                    interactions.push(DrugInteraction {
                        drug1: drugs[i].clone(),
                        drug2: drugs[j].clone(),
                        severity: rule.severity.to_string(),
                        description: rule.description.to_string(),
                        recommendation: rule.recommendation.to_string(),
                    });
                }
            }
        }
    }
    // Deduplicate by (drug1, drug2, description)
    interactions.sort_by(|a, b| {
        a.severity.cmp(&b.severity)
            .then(a.drug1.cmp(&b.drug1))
            .then(a.drug2.cmp(&b.drug2))
    });
    interactions.dedup_by(|a, b| a.drug1 == b.drug1 && a.drug2 == b.drug2 && a.description == b.description);
    interactions
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DrugInteraction {
    pub drug1: String,
    pub drug2: String,
    pub severity: String,
    pub description: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractionCheckResult {
    pub interactions: Vec<DrugInteraction>,
    pub checked_drugs: Vec<String>,
}

#[derive(Deserialize)]
pub struct PatientCheckReq {
    #[serde(default)]
    pub new_drug: Option<String>,
}

#[derive(Deserialize)]
pub struct DrugsCheckReq {
    pub drugs: Vec<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn check_patient_interactions(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<PatientCheckReq>,
) -> Result<Json<ApiResponse<InteractionCheckResult>>, AppError> {
    let db = state.db.clone();
    let meds = run_blocking(move || db.list_medications_by_patient(&patient_id)).await?;

    let mut drugs: Vec<String> = meds.iter().filter(|m| m.active).map(|m| m.name.clone()).collect();

    if let Some(new_drug) = req.new_drug {
        let trimmed = new_drug.trim().to_string();
        if !trimmed.is_empty() && !drugs.contains(&trimmed) {
            drugs.push(trimmed);
        }
    }

    let interactions = check_interactions(&drugs);
    Ok(Json(ApiResponse::ok(
        InteractionCheckResult {
            interactions,
            checked_drugs: drugs,
        },
        "检查完成",
    )))
}

pub async fn check_drugs_interactions(
    Json(req): Json<DrugsCheckReq>,
) -> Result<Json<ApiResponse<InteractionCheckResult>>, AppError> {
    let drugs: Vec<String> = req.drugs.into_iter().filter(|d| !d.trim().is_empty()).collect();
    let interactions = check_interactions(&drugs);
    Ok(Json(ApiResponse::ok(
        InteractionCheckResult {
            interactions,
            checked_drugs: drugs,
        },
        "检查完成",
    )))
}
