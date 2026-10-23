"""Keep complete CPU, wall and memory measurements; never gate on a speed ratio."""

import statistics


def timing_summary(values):
    result = {"n": len(values), "values": values, "status": "inconclusive"}
    if not values:
        return result
    result["median"] = statistics.median(values)
    if len(values) > 1:
        result["sample_cv_percent"] = 100 * statistics.stdev(values) / statistics.mean(values)
    if len(values) == 4 and result.get("sample_cv_percent", 100) <= 5:
        result["status"] = "complete"
    return result


def summarize_results(results, cases):
    summary = {}
    for case in cases:
        arms = {}
        for arm in ("base", "head"):
            selected = [x for x in results if x["case"] == case and x["arm"] == arm]
            inferred = [
                next(r for r in x["records"] if r["stage"] == "inferred")
                for x in selected
                if not x["guard"]
            ]
            arms[arm] = timing_summary([r["inference_cpu_ns"] / 1e9 for r in inferred])
            arms[arm]["wall_seconds"] = timing_summary(
                [r["inference_wall_ns"] / 1e9 for r in inferred]
            )
            arms[arm]["peak_before_export_bytes"] = timing_summary(
                [r["peak_before_export_bytes"] for r in inferred]
            )
        complete = all(value["status"] == "complete" for value in arms.values())
        summary[case] = {"arms": arms, "status": "complete" if complete else "inconclusive"}
        if complete:
            summary[case]["head_over_base_cpu_median"] = (
                arms["head"]["median"] / arms["base"]["median"]
            )
    return summary
