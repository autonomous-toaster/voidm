#![cfg(feature = "ner")]

use voidm_core::ner::{extract_entities, NamedEntity};

/// Golden labels for diverse entity extraction test cases
/// Format: (text, expected_entities_with_types, domain)
/// NOTE: Includes both traditional NER cases (people/orgs/locations) AND
/// technical cases where BERT returns zero entities but tinyllama could help
const GOLDEN_CORPUS: &[(
    &str,
    &[(&str, &str)], // (entity_text, entity_type)
    &str,            // domain
)] = &[
    // TECH domain with known entities (10 examples)
    (
        "Satya Nadella is the CEO of Microsoft in Redmond.",
        &[("Satya Nadella", "PER"), ("Microsoft", "ORG"), ("Redmond", "LOC")],
        "tech",
    ),
    (
        "Google was founded by Larry Page and Sergey Brin in Mountain View.",
        &[
            ("Google", "ORG"),
            ("Larry Page", "PER"),
            ("Sergey Brin", "PER"),
            ("Mountain View", "LOC"),
        ],
        "tech",
    ),
    (
        "Apple Inc. released the iPhone in Cupertino, California.",
        &[("Apple Inc.", "ORG"), ("iPhone", "MISC"), ("Cupertino", "LOC"), ("California", "LOC")],
        "tech",
    ),
    (
        "Netflix CEO Reed Hastings works from Los Gatos.",
        &[("Netflix", "ORG"), ("Reed Hastings", "PER"), ("Los Gatos", "LOC")],
        "tech",
    ),
    (
        "Tesla's Elon Musk operates from Austin, Texas.",
        &[("Tesla", "ORG"), ("Elon Musk", "PER"), ("Austin", "LOC"), ("Texas", "LOC")],
        "tech",
    ),
    (
        "Amazon was founded by Jeff Bezos in Seattle.",
        &[("Amazon", "ORG"), ("Jeff Bezos", "PER"), ("Seattle", "LOC")],
        "tech",
    ),
    (
        "Meta CEO Mark Zuckerberg is based in Menlo Park.",
        &[("Meta", "ORG"), ("Mark Zuckerberg", "PER"), ("Menlo Park", "LOC")],
        "tech",
    ),
    (
        "Nvidia's Jensen Huang leads from Santa Clara.",
        &[("Nvidia", "ORG"), ("Jensen Huang", "PER"), ("Santa Clara", "LOC")],
        "tech",
    ),
    (
        "IBM was once led by Ginni Rometty in Armonk, New York.",
        &[("IBM", "ORG"), ("Ginni Rometty", "PER"), ("Armonk", "LOC"), ("New York", "LOC")],
        "tech",
    ),
    (
        "Intel CEO Pat Gelsinger works in Santa Clara, California.",
        &[("Intel", "ORG"), ("Pat Gelsinger", "PER"), ("Santa Clara", "LOC"), ("California", "LOC")],
        "tech",
    ),
    
    // HEALTHCARE domain (8 examples)
    (
        "Dr. Anthony Fauci works at the National Institutes of Health in Bethesda.",
        &[
            ("Dr. Anthony Fauci", "PER"),
            ("National Institutes of Health", "ORG"),
            ("Bethesda", "LOC"),
        ],
        "healthcare",
    ),
    (
        "Pfizer CEO Albert Bourla is based in New York.",
        &[("Pfizer", "ORG"), ("Albert Bourla", "PER"), ("New York", "LOC")],
        "healthcare",
    ),
    (
        "Moderna's Stéphane Bancel operates from Cambridge, Massachusetts.",
        &[("Moderna", "ORG"), ("Stéphane Bancel", "PER"), ("Cambridge", "LOC"), ("Massachusetts", "LOC")],
        "healthcare",
    ),
    (
        "The Mayo Clinic is headquartered in Rochester, Minnesota.",
        &[("Mayo Clinic", "ORG"), ("Rochester", "LOC"), ("Minnesota", "LOC")],
        "healthcare",
    ),
    (
        "Cleveland Clinic CEO Tom Mihaljevic leads from Ohio.",
        &[("Cleveland Clinic", "ORG"), ("Tom Mihaljevic", "PER"), ("Ohio", "LOC")],
        "healthcare",
    ),
    (
        "Johns Hopkins University Medical Center is in Baltimore, Maryland.",
        &[
            ("Johns Hopkins University Medical Center", "ORG"),
            ("Baltimore", "LOC"),
            ("Maryland", "LOC"),
        ],
        "healthcare",
    ),
    (
        "Dr. Laura Esserman leads UCSF Breast Care Center in San Francisco.",
        &[
            ("Dr. Laura Esserman", "PER"),
            ("UCSF Breast Care Center", "ORG"),
            ("San Francisco", "LOC"),
        ],
        "healthcare",
    ),
    (
        "Harvard Medical School is located in Boston, Massachusetts.",
        &[
            ("Harvard Medical School", "ORG"),
            ("Boston", "LOC"),
            ("Massachusetts", "LOC"),
        ],
        "healthcare",
    ),
    
    // BUSINESS/FINANCE domain (8 examples)
    (
        "Warren Buffett founded Berkshire Hathaway in Omaha, Nebraska.",
        &[
            ("Warren Buffett", "PER"),
            ("Berkshire Hathaway", "ORG"),
            ("Omaha", "LOC"),
            ("Nebraska", "LOC"),
        ],
        "finance",
    ),
    (
        "JPMorgan Chase CEO Jamie Dimon works from New York.",
        &[("JPMorgan Chase", "ORG"), ("Jamie Dimon", "PER"), ("New York", "LOC")],
        "finance",
    ),
    (
        "Goldman Sachs is headquartered in New York City.",
        &[("Goldman Sachs", "ORG"), ("New York City", "LOC")],
        "finance",
    ),
    (
        "BlackRock CEO Laurence Fink leads from New York.",
        &[("BlackRock", "ORG"), ("Laurence Fink", "PER"), ("New York", "LOC")],
        "finance",
    ),
    (
        "Vanguard was founded by John Bogle in Malvern, Pennsylvania.",
        &[
            ("Vanguard", "ORG"),
            ("John Bogle", "PER"),
            ("Malvern", "LOC"),
            ("Pennsylvania", "LOC"),
        ],
        "finance",
    ),
    (
        "Morgan Stanley operates from Midtown Manhattan in New York.",
        &[("Morgan Stanley", "ORG"), ("Midtown Manhattan", "LOC"), ("New York", "LOC")],
        "finance",
    ),
    (
        "Citigroup CEO Jane Fraser is based in New York.",
        &[("Citigroup", "ORG"), ("Jane Fraser", "PER"), ("New York", "LOC")],
        "finance",
    ),
    (
        "Bank of America's headquarters are in Charlotte, North Carolina.",
        &[
            ("Bank of America", "ORG"),
            ("Charlotte", "LOC"),
            ("North Carolina", "LOC"),
        ],
        "finance",
    ),
    
    // RESEARCH/ACADEMIA domain (8 examples)
    (
        "Stanford University in Palo Alto is led by President Jonathan Levin.",
        &[
            ("Stanford University", "ORG"),
            ("Palo Alto", "LOC"),
            ("Jonathan Levin", "PER"),
        ],
        "research",
    ),
    (
        "MIT is located in Cambridge, Massachusetts.",
        &[("MIT", "ORG"), ("Cambridge", "LOC"), ("Massachusetts", "LOC")],
        "research",
    ),
    (
        "Princeton University President Christopher Eisgruber is based in New Jersey.",
        &[
            ("Princeton University", "ORG"),
            ("Christopher Eisgruber", "PER"),
            ("New Jersey", "LOC"),
        ],
        "research",
    ),
    (
        "Yale University is in New Haven, Connecticut.",
        &[("Yale University", "ORG"), ("New Haven", "LOC"), ("Connecticut", "LOC")],
        "research",
    ),
    (
        "UC Berkeley President Carol Christ leads from California.",
        &[("UC Berkeley", "ORG"), ("Carol Christ", "PER"), ("California", "LOC")],
        "research",
    ),
    (
        "Caltech is operated by President Thomas Rosenbaum in Pasadena.",
        &[
            ("Caltech", "ORG"),
            ("Thomas Rosenbaum", "PER"),
            ("Pasadena", "LOC"),
        ],
        "research",
    ),
    (
        "Northwestern University is in Evanston, Illinois.",
        &[("Northwestern University", "ORG"), ("Evanston", "LOC"), ("Illinois", "LOC")],
        "research",
    ),
    (
        "Duke University is located in Durham, North Carolina.",
        &[
            ("Duke University", "ORG"),
            ("Durham", "LOC"),
            ("North Carolina", "LOC"),
        ],
        "research",
    ),
    
    // TECHNICAL domain - BERT returns ZERO (6 examples)
    // These test the fallback case where BERT can't find entities
    (
        "SCRED Proxy - HTTP_PROXY/HTTPS_PROXY/NO_PROXY Environment Variable Support",
        &[("HTTP_PROXY", "MISC"), ("HTTPS_PROXY", "MISC"), ("NO_PROXY", "MISC")],
        "technical",
    ),
    (
        "Set LOG_LEVEL environment variable to DEBUG, INFO, WARN, or ERROR",
        &[("LOG_LEVEL", "MISC"), ("DEBUG", "MISC"), ("INFO", "MISC"), ("WARN", "MISC"), ("ERROR", "MISC")],
        "technical",
    ),
    (
        "REST API endpoints: /api/users GET, POST /api/users, GET /api/users/{id}",
        &[("/api/users", "MISC"), ("/api/users/{id}", "MISC")],
        "technical",
    ),
    (
        "Configuration: db_host, db_port, db_user, db_password in config.yml",
        &[("db_host", "MISC"), ("db_port", "MISC"), ("db_user", "MISC"), ("db_password", "MISC")],
        "technical",
    ),
    (
        "Docker environment: CONTAINER_NAME, IMAGE_TAG, REGISTRY_URL, NETWORK_MODE",
        &[("CONTAINER_NAME", "MISC"), ("IMAGE_TAG", "MISC"), ("REGISTRY_URL", "MISC"), ("NETWORK_MODE", "MISC")],
        "technical",
    ),
    (
        "Python package: setup.py requires numpy>=1.19.0, scipy>=1.5.0, tensorflow==2.10.0",
        &[("numpy", "MISC"), ("scipy", "MISC"), ("tensorflow", "MISC")],
        "technical",
    ),
];

#[derive(Debug, Clone)]
struct ExtractionResult {
    text_preview: String,
    domain: String,
    precision: f32,
    recall: f32,
    f1: f32,
}

fn evaluate_extraction(
    extracted: &[NamedEntity],
    golden: &[(&str, &str)],
    text: &str,
    domain: &str,
) -> ExtractionResult {
    // Normalize extracted entities for comparison
    // Remove trailing punctuation and convert to lowercase
    let extracted_set: std::collections::HashSet<_> = extracted
        .iter()
        .map(|e| {
            let normalized = e.text
                .trim_end_matches(|c: char| !c.is_alphanumeric() && c != ' ')
                .to_lowercase();
            (normalized, e.entity_type.clone())
        })
        .collect();

    let golden_set: std::collections::HashSet<_> = golden
        .iter()
        .map(|(t, ty)| {
            let normalized = t
                .trim_end_matches(|c: char| !c.is_alphanumeric() && c != ' ')
                .to_lowercase();
            (normalized, ty.to_string())
        })
        .collect();

    let true_positives = extracted_set.intersection(&golden_set).count() as f32;
    let false_positives = extracted_set.difference(&golden_set).count() as f32;
    let false_negatives = golden_set.difference(&extracted_set).count() as f32;

    let precision = if true_positives + false_positives > 0.0 {
        true_positives / (true_positives + false_positives)
    } else {
        0.0
    };

    let recall = if true_positives + false_negatives > 0.0 {
        true_positives / (true_positives + false_negatives)
    } else {
        0.0
    };

    let f1 = if precision + recall > 0.0 {
        2.0 * (precision * recall) / (precision + recall)
    } else {
        0.0
    };

    ExtractionResult {
        text_preview: text.chars().take(50).collect(),
        domain: domain.to_string(),
        precision,
        recall,
        f1,
    }
}

#[test]
fn test_ner_quality_honest_measurement() {
    // Ensure model is loaded
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        voidm_core::ner::ensure_ner_model()
            .await
            .expect("Failed to load NER model");
    });

    let mut all_results = Vec::new();
    let mut domain_stats: std::collections::HashMap<&str, (f32, i32)> = std::collections::HashMap::new();

    for (text, golden, domain) in GOLDEN_CORPUS {
        let extracted = extract_entities(text).expect("Extraction failed");
        let result = evaluate_extraction(&extracted, golden, text, domain);

        // Accumulate domain stats
        domain_stats
            .entry(domain)
            .and_modify(|(sum, count)| {
                *sum += result.f1;
                *count += 1;
            })
            .or_insert((result.f1, 1));

        all_results.push(result);
    }

    // Calculate overall metrics
    let total_f1: f32 = all_results.iter().map(|r| r.f1).sum();
    let avg_f1 = total_f1 / all_results.len() as f32;

    let avg_precision: f32 = all_results.iter().map(|r| r.precision).sum::<f32>() / all_results.len() as f32;
    let avg_recall: f32 = all_results.iter().map(|r| r.recall).sum::<f32>() / all_results.len() as f32;

    println!("\n=== NER Quality Measurement ===");
    println!("Total examples: {}", all_results.len());
    println!("Overall F1: {:.4}", avg_f1);
    println!("Overall Precision: {:.4}", avg_precision);
    println!("Overall Recall: {:.4}", avg_recall);
    println!("\nBy Domain:");
    for (domain, (sum, count)) in &domain_stats {
        println!(
            "  {}: {:.4} (n={})",
            domain,
            sum / *count as f32,
            count
        );
    }

    // Ensure minimum quality
    assert!(
        avg_f1 > 0.65,
        "NER quality too low: F1 = {:.4}, expected > 0.65",
        avg_f1
    );

    println!("\n=== Test Output for Autoresearch ===");
    println!("METRIC concept_extraction_quality={:.4}", avg_f1);
    println!("METRIC precision={:.4}", avg_precision);
    println!("METRIC recall={:.4}", avg_recall);
}

#[test]
#[ignore] // Only run when explicitly requested
fn test_ner_quality_verbose() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        voidm_core::ner::ensure_ner_model()
            .await
            .expect("Failed to load NER model");
    });

    let mut results = Vec::new();
    for (text, golden, domain) in GOLDEN_CORPUS {
        let extracted = extract_entities(text).expect("Extraction failed");
        let result = evaluate_extraction(&extracted, golden, text, domain);
        results.push(result);
    }

    println!("\n=== Detailed Results ===");
    for r in &results {
        println!(
            "[{}] {} | F1: {:.4} | P: {:.4} | R: {:.4}",
            r.domain, r.text_preview, r.f1, r.precision, r.recall
        );
    }

    let avg_f1: f32 = results.iter().map(|r| r.f1).sum::<f32>() / results.len() as f32;
    println!("\n=== Overall ===");
    println!("Average F1: {:.4}", avg_f1);
}
