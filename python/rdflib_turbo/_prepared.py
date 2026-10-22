"""Snapshot prepared RDFLib objects as typed algebra, never as SPARQL text."""

import json

from rdflib import BNode, Literal, Variable

from ._operations import _encode


def atom(value):
    if isinstance(value, Variable):
        return {"variable": str(value)}
    return {"term": _encode(value)}


def node(kind, **fields):
    return {"kind": kind, **fields}


def source_variables(scope):
    if scope is None:
        return {"state": "absent"}
    keys = []
    for value in scope:
        if isinstance(value, Variable):
            keys.append({"kind": "variable", "name": str(value)})
        elif isinstance(value, BNode):
            keys.append({"kind": "blank", "label": str(value)})
        elif type(value) is str:
            keys.append({"kind": "string", "value": value})
        else:
            keys.append({"kind": "constant", "term": _encode(value)})
    return {
        "state": "present",
        "keys": sorted(keys, key=lambda key: json.dumps(key, sort_keys=True)),
    }


def expression(value):
    if not hasattr(value, "name"):
        return node("atom", value=atom(value))
    name = value.name
    if name in {"Builtin_EXISTS", "Builtin_NOTEXISTS"}:
        result = node("exists", pattern=pattern(value.graph))
        return node("unary", operator="!", inner=result) if name == "Builtin_NOTEXISTS" else result
    if name == "TrueFilter":
        return node("atom", value=atom(Literal(True)))
    if name == "RelationalExpression":
        return node(
            "binary", operator=value.op, left=expression(value.expr), right=expression(value.other)
        )
    if name in {"ConditionalAndExpression", "ConditionalOrExpression"}:
        result = expression(value.expr)
        for other in value.other:
            result = node(
                "binary",
                operator="&&" if name == "ConditionalAndExpression" else "||",
                left=result,
                right=expression(other),
            )
        return result
    if name in {"UnaryNot", "UnaryPlus", "UnaryMinus"}:
        return node(
            "unary",
            operator={"UnaryNot": "!", "UnaryPlus": "+", "UnaryMinus": "-"}[name],
            inner=expression(value.expr),
        )
    raise NotImplementedError(f"Unsupported prepared expression: {name}")


def empty():
    return node("bgp", triples=[])


def join(left, right, lazy=False):
    return node("join", left=left, right=right, lazy=lazy)


def raw_group(value):
    if value.name == "SubSelect":
        result = raw_group(value.where)
        for field in ("groupby", "having", "orderby", "valuesClause"):
            if getattr(value, field):
                raise NotImplementedError(f"Unsupported prepared raw SubSelect clause: {field}")
        selection = value.projection
        if selection and any(item.expr is not None for item in selection):
            raise NotImplementedError("Unsupported prepared raw SubSelect projection expression")
        variables = (
            [str(item.var) for item in selection]
            if selection
            else sorted(str(v) for v in value._vars)
        )
        result = node("project", inner=result, variables=variables)
        if value.modifier:
            result = node("distinct" if value.modifier == "DISTINCT" else "reduced", inner=result)
        if value.limitoffset:
            limits = value.limitoffset
            result = node(
                "slice",
                inner=result,
                start=int(limits.offset) if limits.offset is not None else 0,
                length=int(limits.limit) if limits.limit is not None else None,
            )
        return result
    result = empty()
    filters = []
    for part in value.part or []:
        name = part.name
        if name == "Filter":
            filters.append(expression(part.expr))
        elif name == "Bind":
            result = node(
                "extend", inner=result, variable=str(part.var), expression=expression(part.expr)
            )
        elif name == "TriplesBlock":
            result = join(
                result, node("bgp", triples=[[atom(t) for t in triple] for triple in part.triples])
            )
        elif name == "GroupOrUnionGraphPattern":
            children = [raw_group(child) for child in part.graph]
            child = children[0]
            for other in children[1:]:
                child = node("union", left=child, right=other)
            result = join(result, child)
        elif name == "OptionalGraphPattern":
            child = raw_group(part.graph)
            guard = node("atom", value=atom(Literal(True)))
            if child["kind"] == "filter":
                guard, child = child["expression"], child["inner"]
            result = node(
                "optional",
                left=result,
                right=child,
                expression=guard,
                variables={"state": "absent"},
            )
        elif name == "MinusGraphPattern":
            result = node("minus", left=result, right=raw_group(part.graph))
        else:
            raise NotImplementedError(f"Unsupported prepared EXISTS syntax: {name}")
    for guard in filters:
        result = node(
            "filter",
            inner=result,
            expression=guard,
            variables={"state": "absent"},
            no_isolated_scope=False,
        )
    return result


def pattern(value):
    if value is None:
        return empty()
    name = value.name
    if name in {"GroupGraphPatternSub", "SubSelect"}:
        return raw_group(value)
    if name == "BGP":
        return node("bgp", triples=[[atom(t) for t in triple] for triple in value.triples])
    if name in {"Join", "LeftJoin", "Union", "Minus"}:
        result = node(
            {"Join": "join", "LeftJoin": "optional", "Union": "union", "Minus": "minus"}[name],
            left=pattern(value.p1),
            right=pattern(value.p2),
        )
        if name == "Join":
            result["lazy"] = bool(value.lazy)
        if name == "LeftJoin":
            result["expression"] = expression(value.expr)
            scope = value.p1._vars
            result["variables"] = source_variables(scope)
        return result
    if name == "ToMultiSet":
        return pattern(value.p)
    if name == "values":
        variables = list(dict.fromkeys(str(v) for row in value.res for v in row))
        return node(
            "values",
            variables=variables,
            rows=[
                [
                    None if row.get(Variable(v), "UNDEF") == "UNDEF" else atom(row[Variable(v)])
                    for v in variables
                ]
                for row in value.res
            ],
        )
    if name in {"Project", "Distinct", "Reduced", "Slice", "Extend", "Filter", "OrderBy"}:
        result = node(name.lower(), inner=pattern(value.p))
        if name == "Project":
            result["variables"] = [str(v) for v in value.PV]
        elif name == "Slice":
            result.update(start=value.start, length=value.length)
        elif name == "Extend":
            result.update(variable=str(value.var), expression=expression(value.expr))
        elif name == "Filter":
            result["expression"] = expression(value.expr)
            result["variables"] = source_variables(value._vars)
            result["no_isolated_scope"] = bool(value.no_isolated_scope)
        elif name == "OrderBy":
            result["expressions"] = [
                {"descending": item.order == "DESC", "expression": expression(item.expr)}
                for item in value.expr
            ]
        return result
    if name == "Graph":
        return node("graph", name=atom(value.term), inner=pattern(value.p))
    raise NotImplementedError(f"Unsupported prepared graph algebra: {name}")


def snapshot(query, *, dataset_context=False):
    from rdflib.plugins.sparql.sparql import Query

    if not isinstance(query, Query):
        raise TypeError("A prepared RDFLib Query is required")
    algebra = query.algebra
    form = {
        "SelectQuery": "select",
        "AskQuery": "ask",
        "ConstructQuery": "construct",
        "DescribeQuery": "describe",
    }.get(algebra.name)
    if form is None:
        raise NotImplementedError(f"Unsupported prepared query form: {algebra.name}")
    clauses = algebra.datasetClause if dataset_context else None
    return json.dumps(
        {
            "form": form,
            "base": str(query.prologue.base) if query.prologue.base else None,
            "pattern": pattern(algebra.p),
            "template": [[atom(t) for t in triple] for triple in algebra.template]
            if form == "construct"
            else [],
            "describe": [atom(t) for t in algebra.PV] if form == "describe" else [],
            "dataset": [
                {
                    "named": not bool(clause.default),
                    "iri": str(clause.default if clause.default else clause.named),
                }
                for clause in clauses or []
            ],
        }
    )
